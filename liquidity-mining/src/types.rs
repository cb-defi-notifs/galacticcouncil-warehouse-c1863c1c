use super::*;
use sp_std::vec;

pub type Balance = u128;
pub type FarmId = u32;
pub type GlobalFarmId = FarmId;
pub type YieldFarmId = FarmId;
pub type FarmMultiplier = FixedU128;
pub type DepositId = u128;

/// This type represent number of live(active and stopped)` yield farms in global farm.
pub type LiveFarmsCount = u32;
/// This type represent number of total(active, stopped and deleted)` yield farms in global farm.
pub type TotalFarmsCount = u32;

/// This struct represents the state a of single liquidity mining program. `YieldFarm`s are rewarded from
/// `GlobalFarm` based on their stake in `GlobalFarm`. `YieldFarm` stake in `GlobalFarm` is derived from
/// users stake in `YieldFarm`.
/// Yield farm is considered live from global farm view if yield farm is `active` or `stopped`.
#[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebugNoBound, TypeInfo)]
#[scale_info(skip_type_params(I))]
pub struct GlobalFarmData<T: Config<I>, I: 'static = ()> {
    pub id: GlobalFarmId,
    pub owner: AccountIdOf<T>,
    pub updated_at: PeriodOf<T>,
    pub total_shares_z: Balance,
    pub accumulated_rpz: Balance,
    pub reward_currency: AssetIdOf<T, I>,
    pub accumulated_rewards: Balance,
    pub paid_accumulated_rewards: Balance,
    pub yield_per_period: Permill,
    pub planned_yielding_periods: PeriodOf<T>,
    pub blocks_per_period: BlockNumberFor<T>,
    pub incentivized_asset: AssetIdOf<T, I>,
    pub max_reward_per_period: Balance,
    //`TotalFarmsCount` includes active, stopped and deleted. Total count is decreased only if yield farms
    //is flushed. `LiveFarmsCount` includes `active` and `stopped` yield farms.
    pub yield_farms_count: (LiveFarmsCount, TotalFarmsCount),
    pub state: GlobalFarmState,
}

impl<T: Config<I>, I: 'static> GlobalFarmData<T, I> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: GlobalFarmId,
        updated_at: PeriodOf<T>,
        reward_currency: T::CurrencyId,
        yield_per_period: Permill,
        planned_yielding_periods: PeriodOf<T>,
        blocks_per_period: T::BlockNumber,
        owner: AccountIdOf<T>,
        incentivized_asset: T::CurrencyId,
        max_reward_per_period: Balance,
    ) -> Self {
        Self {
            accumulated_rewards: Zero::zero(),
            accumulated_rpz: Zero::zero(),
            paid_accumulated_rewards: Zero::zero(),
            total_shares_z: Zero::zero(),
            yield_farms_count: (Zero::zero(), Zero::zero()),
            id,
            updated_at,
            reward_currency,
            yield_per_period,
            planned_yielding_periods,
            blocks_per_period,
            owner,
            incentivized_asset,
            max_reward_per_period,
            state: GlobalFarmState::Active,
        }
    }

    /// This function updates yields_farm_count when new yield farm is added into the global farm.
    /// This function should be called only when new yield farm is created/added into the global
    /// farm.
    pub fn yield_farm_added(&mut self) -> Result<(), ArithmeticError> {
        self.yield_farms_count = (
            self.yield_farms_count
                .0
                .checked_add(1)
                .ok_or(ArithmeticError::Overflow)?,
            self.yield_farms_count
                .1
                .checked_add(1)
                .ok_or(ArithmeticError::Overflow)?,
        );

        Ok(())
    }

    /// This function updates `yield_farms_count` when yield farm is removed from global farm.
    /// This function should be called only when yield farm is removed from global farm.
    pub fn yield_farm_removed(&mut self) -> Result<(), ArithmeticError> {
        //Note: only live count should change
        self.yield_farms_count.0 = self
            .yield_farms_count
            .0
            .checked_sub(1)
            .ok_or(ArithmeticError::Underflow)?;

        Ok(())
    }

    /// This function updates `yield_farms_count` when yield farm is flushed from storage.
    /// This function should be called only if yield farm is flushed.
    /// DON'T call this function if yield farm is in stopped or deleted state.
    pub fn yield_farm_flushed(&mut self) -> Result<(), DispatchError> {
        self.yield_farms_count.1 = self
            .yield_farms_count
            .1
            .checked_sub(1)
            .ok_or(ArithmeticError::Underflow)?;

        Ok(())
    }

    /// Function returns `true` if global farm has no live yield farms.
    pub fn has_no_live_farms(&self) -> bool {
        self.yield_farms_count.0.is_zero()
    }

    /// Function return `true` if global farm can be flushed(removed) from storage.
    pub fn can_be_flushed(&self) -> bool {
        //farm can be flushed only if all yield farms are flushed.
        self.state == GlobalFarmState::Deleted && self.yield_farms_count.1.is_zero()
    }

    /// Function return `true` if global farm is in active state.
    pub fn is_active(&self) -> bool {
        self.state == GlobalFarmState::Active
    }
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebugNoBound, TypeInfo)]
#[scale_info(skip_type_params(I))]
pub struct YieldFarmData<T: Config<I>, I: 'static = ()> {
    pub id: FarmId,
    pub updated_at: PeriodOf<T>,
    pub total_shares: Balance,
    pub total_valued_shares: Balance,
    pub accumulated_rpvs: Balance,
    pub accumulated_rpz: Balance,
    pub loyalty_curve: Option<LoyaltyCurve>,
    pub multiplier: FarmMultiplier,
    pub state: YieldFarmState,
    pub entries_count: u64,
    pub _phantom: PhantomData<I>, //pub because of tests
}

impl<T: Config<I>, I: 'static> YieldFarmData<T, I> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        id: FarmId,
        updated_at: PeriodOf<T>,
        loyalty_curve: Option<LoyaltyCurve>,
        multiplier: FarmMultiplier,
    ) -> Self {
        Self {
            id,
            updated_at,
            loyalty_curve,
            multiplier,
            accumulated_rpvs: Zero::zero(),
            accumulated_rpz: Zero::zero(),
            total_shares: Zero::zero(),
            total_valued_shares: Zero::zero(),
            state: YieldFarmState::Active,
            entries_count: Default::default(),
            _phantom: PhantomData::default(),
        }
    }

    /// Function returns `true` if yield farm is in active state.
    pub fn is_active(&self) -> bool {
        self.state == YieldFarmState::Active
    }

    /// Function returns `true` if yield farm is in stopped state.
    pub fn is_stopped(&self) -> bool {
        self.state == YieldFarmState::Stopped
    }

    /// Function returns `true` if yield farm is in deleted state.
    pub fn is_deleted(&self) -> bool {
        self.state == YieldFarmState::Deleted
    }

    /// Returns `true` if yield farm can be removed from storage, `false` otherwise.
    pub fn can_be_flushed(&self) -> bool {
        self.state == YieldFarmState::Deleted && self.entries_count.is_zero()
    }

    /// This function updates entries count in the yield farm. This function should be called if  
    /// entry is removed from the yield farm.
    pub fn entry_removed(&mut self) -> Result<(), ArithmeticError> {
        self.entries_count = self.entries_count.checked_sub(1).ok_or(ArithmeticError::Underflow)?;

        Ok(())
    }

    /// This function updates entries count in the yield farm. This function should be called if
    /// entry is added into the yield farm.
    pub fn entry_added(&mut self) -> Result<(), ArithmeticError> {
        self.entries_count = self.entries_count.checked_add(1).ok_or(ArithmeticError::Overflow)?;

        Ok(())
    }

    /// This function return `true` if yield farm is empty.
    pub fn has_entries(&self) -> bool {
        !self.entries_count.is_zero()
    }
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(I))]
pub struct LoyaltyCurve {
    pub initial_reward_percentage: FixedU128,
    pub scale_coef: u32,
}

impl Default for LoyaltyCurve {
    fn default() -> Self {
        Self {
            initial_reward_percentage: FixedU128::from_inner(500_000_000_000_000_000), // 0.5
            scale_coef: 100,
        }
    }
}

#[derive(Clone, Encode, Decode, RuntimeDebugNoBound, TypeInfo, PartialEq)]
#[scale_info(skip_type_params(I))]
pub struct DepositData<T: Config<I>, I: 'static = ()> {
    pub shares: Balance,
    pub amm_pool_id: T::AmmPoolId,
    //NOTE: Capacity of this vector MUST BE at least 1.
    pub yield_farm_entries: Vec<YieldFarmEntry<T, I>>,
}

impl<T: Config<I>, I: 'static> DepositData<T, I> {
    pub fn new(shares: Balance, amm_pool_id: T::AmmPoolId) -> Self {
        Self {
            shares,
            amm_pool_id,
            //NOTE: Capacity of this vector MUST BE at least 1.
            yield_farm_entries: vec![],
        }
    }

    /// This function add new yield farm entry into the deposit.
    /// This function returns error if deposit reached max entries in the deposit or
    /// `entry.yield_farm_id` is not unique.
    pub fn add_yield_farm_entry(&mut self, entry: YieldFarmEntry<T, I>) -> Result<(), DispatchError> {
        let len = TryInto::<u8>::try_into(self.yield_farm_entries.len()).map_err(|_e| ArithmeticError::Overflow)?;
        if len >= T::MaxFarmEntriesPerDeposit::get() {
            return Err(Error::<T, I>::MaxEntriesPerDeposit.into());
        }

        let idx = match self
            .yield_farm_entries
            .binary_search_by(|e| e.yield_farm_id.cmp(&entry.yield_farm_id))
        {
            Ok(_) => return Err(Error::<T, I>::DoubleLock.into()),
            Err(idx) => idx,
        };

        self.yield_farm_entries.insert(idx, entry);

        Ok(())
    }

    /// This function remove yield farm entry from the deposit. This function returns error if
    /// yield farm entry in not found in the deposit.
    pub fn remove_yield_farm_entry(&mut self, yield_farm_id: YieldFarmId) -> Result<YieldFarmEntry<T, I>, Error<T, I>> {
        let idx = match self
            .yield_farm_entries
            .binary_search_by(|e| e.yield_farm_id.cmp(&yield_farm_id))
        {
            Ok(idx) => idx,
            Err(_) => return Err(Error::<T, I>::YieldFarmEntryNotFound),
        };

        Ok(self.yield_farm_entries.remove(idx))
    }

    /// This function return yield farm entry from deposit of `None` if yield farm entry is not
    /// found.
    pub fn get_yield_farm_entry(&mut self, yield_farm_id: FarmId) -> Option<&mut YieldFarmEntry<T, I>> {
        match self
            .yield_farm_entries
            .binary_search_by(|e| e.yield_farm_id.cmp(&yield_farm_id))
        {
            Ok(idx) => self.yield_farm_entries.get_mut(idx),
            Err(_) => None,
        }
    }

    /// This function returns `true` if deposit contains yield farm entry with given yield farm id.
    pub fn contains_yield_farm_entry(&self, yield_farm_id: YieldFarmId) -> bool {
        self.yield_farm_entries
            .binary_search_by(|e| e.yield_farm_id.cmp(&yield_farm_id))
            .is_ok()
    }

    /// This function returns `true` if deposit has no yield farm entries.
    pub fn has_no_yield_farm_entries(&self) -> bool {
        self.yield_farm_entries.is_empty()
    }

    /// This function returns `true` if deposit can be flushed from storage.
    pub fn can_be_flushed(&self) -> bool {
        //NOTE: deposit with no entries should/must be flushed
        self.has_no_yield_farm_entries()
    }
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(I))]
pub struct YieldFarmEntry<T: Config<I>, I: 'static = ()> {
    pub global_farm_id: GlobalFarmId,
    pub yield_farm_id: YieldFarmId,
    pub valued_shares: Balance,
    pub accumulated_rpvs: Balance,
    pub accumulated_claimed_rewards: Balance,
    pub entered_at: PeriodOf<T>,
    pub updated_at: PeriodOf<T>,
    pub _phantom: PhantomData<I>, //pub because of tests
}

impl<T: Config<I>, I: 'static> YieldFarmEntry<T, I> {
    pub fn new(
        global_farm_id: GlobalFarmId,
        yield_farm_id: YieldFarmId,
        valued_shares: Balance,
        accumulated_rpvs: Balance,
        entered_at: PeriodOf<T>,
    ) -> Self {
        Self {
            global_farm_id,
            yield_farm_id,
            valued_shares,
            accumulated_rpvs,
            accumulated_claimed_rewards: Zero::zero(),
            entered_at,
            updated_at: entered_at,
            _phantom: PhantomData,
        }
    }
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebugNoBound, TypeInfo)]
pub enum GlobalFarmState {
    Active,
    Deleted,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebugNoBound, TypeInfo)]
pub enum YieldFarmState {
    Active,
    Stopped,
    Deleted,
}
