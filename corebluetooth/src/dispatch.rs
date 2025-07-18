//! Types for working with Grand Central Dispatch (GCD).

use dispatch_executor::DispatchQueueAttrBuilder;
use dispatch2::{DispatchAutoReleaseFrequency, DispatchQueueAttr, DispatchRetained};

/// A quality-of-service level for a dispatch queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DispatchQoS {
    class: dispatch2::DispatchQoS,
    relative_priority: i32,
}

impl Default for DispatchQoS {
    fn default() -> Self {
        Self {
            class: dispatch2::DispatchQoS::Unspecified,
            relative_priority: 0,
        }
    }
}

impl DispatchQoS {
    /// Creates a new quality-of-service level.
    pub fn new(class: dispatch2::DispatchQoS, relative_priority: i32) -> Self {
        Self {
            class,
            relative_priority,
        }
    }

    pub(crate) fn to_attr(self) -> Option<DispatchRetained<DispatchQueueAttr>> {
        DispatchQueueAttrBuilder::serial()
            .with_autorelease_frequency(DispatchAutoReleaseFrequency::WORK_ITEM)
            .with_qos_class(self.class, self.relative_priority)
            .build()
    }
}
