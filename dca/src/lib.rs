#![allow(warnings)]
// This file is part of pallet-dca.

// Copyright (C) 2020-2022  Intergalactic, Limited (GIB).
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::ensure;
use frame_support::traits::fungibles::Inspect;
use frame_support::traits::Get;
use frame_support::transactional;
use frame_system::ensure_signed;
use frame_system::pallet_prelude::OriginFor;
use orml_traits::arithmetic::{CheckedAdd, CheckedSub};
use scale_info::TypeInfo;
use sp_runtime::traits::Saturating;
use sp_runtime::traits::{BlockNumberProvider, ConstU32};
use sp_runtime::ArithmeticError;
use sp_runtime::{BoundedVec, DispatchError};
use sp_std::vec::Vec;

#[cfg(test)]
mod tests;

pub mod types;
pub mod weights;

use weights::WeightInfo;

// Re-export pallet items so that they can be accessed from the crate namespace.
use crate::types::{AssetId, Balance, BlockNumber, ScheduleId};
pub use pallet::*;

const MAX_NUMBER_OF_TRADES: u32 = 5;
const MAX_NUMBER_OF_SCHEDULES_PER_BLOCK: u32 = 20; //TODO: use config for this

type BlockNumberFor<T> = <T as frame_system::Config>::BlockNumber;

#[derive(Encode, Decode, Debug, Eq, PartialEq, Clone, TypeInfo, MaxEncodedLen)]
pub enum Recurrence {
    Fixed,
    Perpetual,
}

#[derive(Encode, Decode, Debug, Eq, PartialEq, Clone, TypeInfo, MaxEncodedLen)]
pub struct Order {
    pub asset_in: Balance,
    pub asset_out: Balance,
    pub amount_in: Balance,
    pub amount_out: Balance,
    pub limit: Balance,
    pub route: BoundedVec<Trade, sp_runtime::traits::ConstU32<MAX_NUMBER_OF_TRADES>>,
}

#[derive(Encode, Decode, Debug, Eq, PartialEq, Clone, TypeInfo, MaxEncodedLen)]
pub struct Schedule {
    pub period: BlockNumber, //TODO: use proper block number
    pub recurrence: Recurrence,
    pub order: Order,
}

///A single trade for buy/sell, describing the asset pair and the pool type in which the trade is executed
#[derive(Encode, Decode, Debug, Eq, PartialEq, Clone, TypeInfo, MaxEncodedLen)]
pub struct Trade {
    pub pool: PoolType, //TODO: consider using the same type as in route executor
    pub asset_in: AssetId,
    pub asset_out: AssetId,
}

#[derive(Encode, Decode, Clone, Copy, Debug, Eq, PartialEq, TypeInfo, MaxEncodedLen)]
pub enum PoolType {
    XYK,
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use codec::EncodeLike;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::OriginFor;
    use hydradx_traits::router::ExecutorError;
    use sp_runtime::traits::{MaybeDisplay, Saturating};
    use std::fmt::Debug;

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

        /// Weight information for the extrinsics.
        type WeightInfo: WeightInfo;
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(crate) fn deposit_event)]
    pub enum Event<T: Config> {
        ///First event
        DummyEvent {},
    }

    #[pallet::error]
    pub enum Error<T> {
        ///First error
        DummyError,
    }

    /// Id sequencer for schedules
    #[pallet::storage]
    #[pallet::getter(fn next_schedule_id)]
    pub type ScheduleIdSequencer<T: Config> = StorageValue<_, ScheduleId, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn schedules)]
    pub type Schedules<T: Config> = StorageMap<_, Blake2_128Concat, ScheduleId, Schedule, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn schedule_ownership)]
    pub type ScheduleOwnership<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, ScheduleId, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn schedule_ids_per_block)]
    pub type ScheduleIdsPerBlock<T: Config> =
        StorageMap<_, Blake2_128Concat, BlockNumberFor<T>, BoundedVec<ScheduleId, ConstU32<20>>, OptionQuery>;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        ///Schedule
        #[pallet::weight(<T as Config>::WeightInfo::sell(5))]
        #[transactional]
        pub fn schedule(
            origin: OriginFor<T>,
            schedule: Schedule,
            next_execution_block: Option<BlockNumberFor<T>>,
        ) -> DispatchResult {
            let who = ensure_signed(origin.clone())?;

            let next_schedule_id = Self::get_next_schedule_id()?;
            Schedules::<T>::insert(next_schedule_id, schedule);
            ScheduleOwnership::<T>::insert(who, next_schedule_id);

            let blocknumber_for_schedule = next_execution_block.unwrap_or_else(|| Self::get_next_block_mumber());

            if !ScheduleIdsPerBlock::<T>::contains_key(blocknumber_for_schedule) {
                let vec_with_first_schedule_id = Self::create_bounded_vec(next_schedule_id);
                ScheduleIdsPerBlock::<T>::insert(blocknumber_for_schedule, vec_with_first_schedule_id);
            } else {
                ScheduleIdsPerBlock::<T>::try_mutate_exists(
                    blocknumber_for_schedule,
                    |schedule_ids| -> DispatchResult {
                        let mut schedule_ids = schedule_ids.as_mut().ok_or(Error::<T>::DummyError)?;

                        schedule_ids
                            .try_push(next_schedule_id)
                            .map_err(|_| Error::<T>::DummyError)?;
                        Ok(())
                    },
                );
            }

            Ok(())
        }
    }
}

impl<T: Config> Pallet<T> {
    fn get_next_schedule_id() -> Result<ScheduleId, ArithmeticError> {
        ScheduleIdSequencer::<T>::try_mutate(|current_id| {
            *current_id = current_id.checked_add(1).ok_or(ArithmeticError::Overflow)?;

            Ok(*current_id)
        })
    }

    fn get_next_block_mumber() -> BlockNumberFor<T> {
        let mut current_block_number = frame_system::Pallet::<T>::current_block_number();
        current_block_number.saturating_inc();

        current_block_number
    }

    fn create_bounded_vec(next_schedule_id: ScheduleId) -> BoundedVec<ScheduleId, ConstU32<20>> {
        let schedule_id = vec![next_schedule_id];
        let bounded_vec: BoundedVec<ScheduleId, ConstU32<20>> = schedule_id.try_into().unwrap(); //TODO: here use constant instead of hardcoded value
        bounded_vec
    }
}
