// This file is part of pallet-ema-oracle.

// Copyright (C) 2022  Intergalactic, Limited (GIB).
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

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::sp_runtime::{FixedU128, RuntimeDebug};
use hydra_dx_math::ema::{balance_ema, exp_smoothing, price_ema, smoothing_from_period, volume_ema};
use hydradx_traits::{AggregatedEntry, Volume};
use scale_info::TypeInfo;
use sp_arithmetic::{
    traits::{AtLeast32BitUnsigned, One, SaturatedConversion, UniqueSaturatedInto, Zero},
    FixedPointNumber,
};

pub use hydradx_traits::Source;

use sp_std::prelude::*;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

pub type AssetId = u32;
pub type Balance = u128;
pub type Price = FixedU128;

/// A type representing data produced by a trade or liquidity event. Timestamped to the block where
/// it was created.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(RuntimeDebug, Encode, Decode, Clone, PartialEq, Eq, Default, TypeInfo, MaxEncodedLen)]
pub struct OracleEntry<BlockNumber> {
    pub price: Price,
    pub volume: Volume<Balance>,
    pub liquidity: Balance,
    pub timestamp: BlockNumber,
}

impl<BlockNumber> OracleEntry<BlockNumber>
where
    BlockNumber: AtLeast32BitUnsigned + Copy + UniqueSaturatedInto<u64>,
{
    /// Convert the `OracleEntry` into an `AggregatedEntry` for consumption. Determines the age by
    /// subtracting `initialized` from the timestamp.
    pub fn into_aggregated(self, initialized: BlockNumber) -> AggregatedEntry<Balance, BlockNumber, Price> {
        AggregatedEntry {
            price: self.price,
            volume: self.volume,
            liquidity: self.liquidity,
            oracle_age: self.timestamp.saturating_sub(initialized),
        }
    }

    /// Return an inverted version of the entry where the meaning of assets a and b are inverted.
    /// So the price of a/b become the price b/a and the volume switches correspondingly.
    pub fn inverted(&self) -> Self {
        // It makes sense for the reciprocal of zero to be zero here.
        let price = self.price.reciprocal().unwrap_or_else(Zero::zero);
        let volume = self.volume.inverted();
        Self {
            price,
            volume,
            liquidity: self.liquidity,
            timestamp: self.timestamp,
        }
    }

    /// Update the volume in `self` by adding in the volume of `incoming` and taking over the other
    /// values.
    pub fn accumulate_volume_and_update_from(&mut self, incoming: &Self) {
        self.volume = incoming.volume.saturating_add(&self.volume);
        self.price = incoming.price;
        self.liquidity = incoming.liquidity;
        self.timestamp = incoming.timestamp;
    }

    /// Determine a new entry based on `self` and a previous entry. Adds the volumes together and
    /// takes the values of `self` for the rest.
    pub fn with_added_volume_from(&self, previous_entry: &Self) -> Self {
        let volume = previous_entry.volume.saturating_add(&self.volume);
        Self {
            price: self.price,
            volume,
            liquidity: self.liquidity,
            timestamp: self.timestamp,
        }
    }

    /// Determine a new oracle entry based on a previous (`self`) and an `incoming` entry as well as
    /// a `period`.
    ///
    /// Returns `None` if any of the calculations fail (including the `incoming` entry not being
    /// more recent than `self`).
    ///
    /// The period is used to determine the smoothing factor alpha for an exponential moving
    /// average. The smoothing factor is calculated as `alpha = 2 / (period + 1)`. `alpha = 2 /
    /// (period + 1)` leads to the "center of mass" of the EMA corresponding to a period-length SMA.
    ///
    /// Uses the difference between the `timestamp`s to determine the time to cover and
    /// exponentiates the complement (`1 - alpha`) with that time difference.
    pub fn combine_via_ema_with(&self, period: BlockNumber, incoming: &Self) -> Option<Self> {
        let iterations = incoming.timestamp.checked_sub(&self.timestamp)?;
        if iterations.is_zero() {
            return None;
        }
        if period <= One::one() {
            return Some(incoming.clone());
        }
        // determine smoothing factor
        let smoothing = smoothing_from_period(period.saturated_into::<u64>());
        let (exp_smoothing, exp_complement) = exp_smoothing(smoothing, iterations.saturated_into::<u32>());

        let price = price_ema(self.price, exp_complement, incoming.price, exp_smoothing);
        let volume = volume_ema(
            self.volume.clone().into(),
            exp_complement,
            incoming.volume.clone().into(),
            exp_smoothing,
        )
        .into();
        let liquidity = balance_ema(self.liquidity, exp_complement, incoming.liquidity, exp_smoothing);

        Some(Self {
            price,
            volume,
            liquidity,
            timestamp: incoming.timestamp,
        })
    }

    /// Update `self` based on a previous (`self`) and an `incoming` entry as well as a `period`.
    ///
    /// Returns `None` if any of the calculations fail (including the `incoming` entry not being
    /// more recent than `self`) or - on success - a reference to `self` for chaining. Use
    /// [`update_via_ema_with`] if you only care about success and failure without chaining.
    ///
    /// The period is used to determine the smoothing factor alpha for an exponential moving
    /// average. The smoothing factor is calculated as `alpha = 2 / (period + 1)`. `alpha = 2 /
    /// (period + 1)` leads to the "center of mass" of the EMA corresponding to a period-length SMA.
    ///
    /// Uses the difference between the `timestamp`s to determine the time to cover and
    /// exponentiates the complement (`1 - alpha`) with that time difference.
    pub fn chained_update_via_ema_with(&mut self, period: BlockNumber, incoming: &Self) -> Option<&mut Self> {
        *self = self.combine_via_ema_with(period, incoming)?;
        Some(self)
    }

    /// Update `self` based on a previous (`self`) and an `incoming` entry as well as a `period`.
    ///
    /// Returns `None` if any of the calculations fail (including the `incoming` entry not being
    /// more recent than `self`) or `()` on success.
    ///
    /// The period is used to determine the smoothing factor alpha for an exponential moving
    /// average. The smoothing factor is calculated as `alpha = 2 / (period + 1)`. `alpha = 2 /
    /// (period + 1)` leads to the "center of mass" of the EMA corresponding to a period-length SMA.
    ///
    /// Uses the difference between the `timestamp`s to determine the time to cover and
    /// exponentiates the complement (`1 - alpha`) with that time difference.
    pub fn update_via_ema_with(&mut self, period: BlockNumber, incoming: &Self) -> Option<()> {
        self.chained_update_via_ema_with(period, incoming).map(|_| ())
    }
}

impl<BlockNumber> From<(Price, Volume<Balance>, Balance, BlockNumber)> for OracleEntry<BlockNumber> {
    fn from((price, volume, liquidity, timestamp): (Price, Volume<Balance>, Balance, BlockNumber)) -> Self {
        Self {
            price,
            volume,
            liquidity,
            timestamp,
        }
    }
}
