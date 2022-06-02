// This file is part of Basilisk-node.

// Copyright (C) 2020-2021  Intergalactic, Limited (GIB).
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

use super::*;
use test_ext::*;

#[test]
fn destroy_global_farm_should_work() {
    //test with flushing - global farm should be removed from storage if it has no yield farms.
    predefined_test_ext().execute_with(|| {
        let farm_account = LiquidityMining::farm_account_id(BOB_FARM).unwrap();
        let bob_reward_currency_balance = Tokens::free_balance(PREDEFINED_GLOBAL_FARMS[1].reward_currency, &BOB);
        let undistributed_rewards = Tokens::free_balance(PREDEFINED_GLOBAL_FARMS[1].reward_currency, &farm_account);

        assert_eq!(
            LiquidityMining::destroy_global_farm(BOB, BOB_FARM).unwrap(),
            (PREDEFINED_GLOBAL_FARMS[1].reward_currency, undistributed_rewards, BOB)
        );

        //global farm with no yield farms should be flushed
        assert!(LiquidityMining::global_farm(BOB_FARM).is_none());

        //undistriburted rewards should be transfered to owner
        assert_eq!(
            Tokens::free_balance(PREDEFINED_GLOBAL_FARMS[1].reward_currency, &BOB),
            bob_reward_currency_balance + undistributed_rewards
        );
    });

    //withouth flushing - global farm should stay in the storage marked as deleted.
    predefined_test_ext().execute_with(|| {
        let farm_account = LiquidityMining::farm_account_id(CHARLIE_FARM).unwrap();
        let charlie_reward_currency_balance =
            Tokens::free_balance(PREDEFINED_GLOBAL_FARMS[3].reward_currency, &CHARLIE);
        let undistributed_rewards = Tokens::free_balance(PREDEFINED_GLOBAL_FARMS[3].reward_currency, &farm_account);
        let yield_farm_id = PREDEFINED_YIELD_FARMS.with(|v| v[2].id.clone());

        //add deposit to yield farm so it will not be flushed on destroy
        assert_ok!(LiquidityMining::deposit_lp_shares(
            BOB,
            CHARLIE_FARM,
            yield_farm_id,
            ACA_KSM_AMM,
            1_000
        ));

        //stop farming
        assert_ok!(LiquidityMining::stop_yield_farm(CHARLIE, CHARLIE_FARM, ACA_KSM_AMM));

        //destory yield farm (yield farm is destroyed but not flushed)
        assert_ok!(LiquidityMining::destroy_yield_farm(
            CHARLIE,
            CHARLIE_FARM,
            yield_farm_id,
            ACA_KSM_AMM
        ));

        //destory global farm
        assert_ok!(LiquidityMining::destroy_global_farm(CHARLIE, CHARLIE_FARM));

        //global farm with yield farms should NOT be flushed
        assert_eq!(
            LiquidityMining::global_farm(CHARLIE_FARM).unwrap(),
            GlobalFarmData {
                yield_farms_count: (0, 1),
                state: GlobalFarmState::Deleted,
                ..PREDEFINED_GLOBAL_FARMS[3]
            }
        );

        assert_eq!(
            Tokens::free_balance(PREDEFINED_GLOBAL_FARMS[3].reward_currency, &CHARLIE),
            charlie_reward_currency_balance + undistributed_rewards
        );
    })
}

#[test]
fn destroy_global_farm_not_owner_should_not_work() {
    predefined_test_ext().execute_with(|| {
        assert_noop!(
            LiquidityMining::destroy_global_farm(ALICE, BOB_FARM),
            Error::<Test>::Forbidden
        );

        assert_eq!(
            LiquidityMining::global_farm(BOB_FARM).unwrap(),
            PREDEFINED_GLOBAL_FARMS[1]
        );
    });
}

#[test]
fn destroy_global_farm_farm_not_exists_should_not_work() {
    predefined_test_ext().execute_with(|| {
        const NON_EXISTING_FARM: u32 = 999_999_999;
        assert_noop!(
            LiquidityMining::destroy_global_farm(ALICE, NON_EXISTING_FARM),
            Error::<Test>::GlobalFarmNotFound
        );
    });
}

#[test]
fn destroy_global_farm_with_yield_farms_should_not_work() {
    //Glboal farm CAN'T be destroyed if it has active or stopped yield farms
    predefined_test_ext().execute_with(|| {
        let yield_farm_id = PREDEFINED_YIELD_FARMS.with(|v| v[2].id.clone());
        assert_eq!(
            LiquidityMining::active_yield_farm(ACA_KSM_AMM, CHARLIE_FARM).unwrap(),
            yield_farm_id
        );

        assert_noop!(
            LiquidityMining::destroy_global_farm(CHARLIE, CHARLIE_FARM),
            Error::<Test>::GlobalFarmIsNotEmpty
        );

        assert_eq!(
            LiquidityMining::global_farm(CHARLIE_FARM).unwrap(),
            PREDEFINED_GLOBAL_FARMS[3]
        );

        //destory farm with stopped farm should not work
        //stop yield farm
        assert_ok!(LiquidityMining::stop_yield_farm(CHARLIE, CHARLIE_FARM, ACA_KSM_AMM));
        assert!(LiquidityMining::active_yield_farm(ACA_KSM_AMM, CHARLIE_FARM).is_none());

        assert_noop!(
            LiquidityMining::destroy_global_farm(CHARLIE, CHARLIE_FARM),
            Error::<Test>::GlobalFarmIsNotEmpty
        );

        assert_eq!(
            LiquidityMining::global_farm(CHARLIE_FARM).unwrap(),
            PREDEFINED_GLOBAL_FARMS[3]
        );
    });
}

#[test]
fn destroy_global_farm_healthy_farm_should_not_work() {
    //farm with undistributed rewards and yield farms
    predefined_test_ext().execute_with(|| {
        let farm_account = LiquidityMining::farm_account_id(GC_FARM).unwrap();
        assert!(!Tokens::free_balance(PREDEFINED_GLOBAL_FARMS[2].reward_currency, &farm_account).is_zero());

        assert_noop!(
            LiquidityMining::destroy_global_farm(GC, GC_FARM),
            Error::<Test>::GlobalFarmIsNotEmpty
        );

        assert_eq!(
            LiquidityMining::global_farm(GC_FARM).unwrap(),
            PREDEFINED_GLOBAL_FARMS[2]
        );
    });
}
