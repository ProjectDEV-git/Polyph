//! The hosted relay's plan policy: a room's limits come from the plan of the
//! account its owner device is linked to. Devices with no account get Free.
//!
//! The relay asks synchronously on every subscribe, so plans are kept in
//! memory and rebuilt from the database whenever accounts, teams or
//! subscriptions change.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use parking_lot::RwLock;
use polyph_proto::ids::DeviceId;
use polyph_relay::Limits;

use crate::db::Db;
use crate::plans::{PlanConfig, PlanId};

#[derive(Debug, Clone)]
pub struct CloudPolicy {
    plans: Arc<BTreeMap<PlanId, PlanConfig>>,
    devices: Arc<RwLock<HashMap<DeviceId, PlanId>>>,
}

impl CloudPolicy {
    pub fn new(plans: BTreeMap<PlanId, PlanConfig>) -> Self {
        Self { plans: Arc::new(plans), devices: Arc::default() }
    }

    pub fn plan_of(&self, device: &DeviceId) -> PlanId {
        self.devices.read().get(device).copied().unwrap_or(PlanId::Free)
    }

    pub fn limits(&self, plan: PlanId) -> Limits {
        self.plans.get(&plan).map(PlanConfig::limits).unwrap_or_default()
    }

    /// Recomputes every linked device's plan.
    pub async fn refresh(&self, db: &Db) -> anyhow::Result<()> {
        let mut plans: HashMap<String, PlanId> = HashMap::new();
        let mut next = HashMap::new();
        for (device, account) in db.all_devices().await? {
            let plan = match plans.get(&account) {
                Some(p) => *p,
                None => {
                    let p = db.effective_plan(&account).await?;
                    plans.insert(account, p);
                    p
                }
            };
            next.insert(device, plan);
        }
        *self.devices.write() = next;
        Ok(())
    }
}

impl polyph_relay::Policy for CloudPolicy {
    fn limits_for_room(&self, owner: Option<&DeviceId>) -> Limits {
        let plan = owner.map(|o| self.plan_of(o)).unwrap_or(PlanId::Free);
        self.limits(plan)
    }
}
