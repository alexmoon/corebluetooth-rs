use std::collections::HashMap;

use btuuid::BluetoothUuid;
use objc2::runtime::AnyObject;
use objc2_core_bluetooth::{
    CBAdvertisementDataIsConnectable, CBAdvertisementDataLocalNameKey,
    CBAdvertisementDataManufacturerDataKey, CBAdvertisementDataOverflowServiceUUIDsKey,
    CBAdvertisementDataServiceDataKey, CBAdvertisementDataServiceUUIDsKey,
    CBAdvertisementDataSolicitedServiceUUIDsKey, CBAdvertisementDataTxPowerLevelKey, CBUUID,
};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSNumber, NSString};

/// Data included in a Bluetooth advertisement or scan reponse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvertisementData {
    /// The (possibly shortened) local name of the device (CSS §A.1.2)
    pub local_name: Option<String>,
    /// Manufacturer specific data (CSS §A.1.4)
    pub manufacturer_data: Option<ManufacturerData>,
    /// Service associated data (CSS §A.1.11)
    pub service_data: HashMap<BluetoothUuid, Vec<u8>>,
    /// Advertised GATT service UUIDs (CSS §A.1.1)
    pub service_uuids: Vec<BluetoothUuid>,
    pub overflow_service_uuids: Vec<BluetoothUuid>,
    /// Transmitted power level (CSS §A.1.5)
    pub tx_power_level: Option<i16>,
    /// Set to true for connectable advertising packets
    pub is_connectable: bool,
    /// Solicited GATT service UUIDs (CSS §A.1.10)
    pub solicited_service_uuids: Vec<BluetoothUuid>,
}

/// Manufacturer specific data included in Bluetooth advertisements. See the Bluetooth Core Specification Supplement
/// §A.1.4 for details.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ManufacturerData {
    /// Company identifier (defined [here](https://www.bluetooth.com/specifications/assigned-numbers/company-identifiers/))
    pub company_id: u16,
    /// Manufacturer specific data
    pub data: Vec<u8>,
}

impl AdvertisementData {
    pub(crate) fn from_nsdictionary(adv_data: &NSDictionary<NSString, AnyObject>) -> Self {
        let is_connectable = adv_data
            .objectForKey(unsafe { CBAdvertisementDataIsConnectable })
            .is_some_and(|val| {
                val.downcast_ref::<NSNumber>()
                    .map(|b| b.as_bool())
                    .unwrap_or(false)
            });

        let local_name = adv_data
            .objectForKey(unsafe { CBAdvertisementDataLocalNameKey })
            .and_then(|val| val.downcast_ref::<NSString>().map(|s| s.to_string()));

        let manufacturer_data = adv_data
            .objectForKey(unsafe { CBAdvertisementDataManufacturerDataKey })
            .and_then(|val| val.downcast_ref::<NSData>().map(|v| v.to_vec()))
            .and_then(|val| {
                (val.len() >= 2).then(|| ManufacturerData {
                    company_id: u16::from_le_bytes(val[0..2].try_into().unwrap()),
                    data: val[2..].to_vec(),
                })
            });

        let tx_power_level: Option<i16> = adv_data
            .objectForKey(unsafe { CBAdvertisementDataTxPowerLevelKey })
            .and_then(|val| val.downcast_ref::<NSNumber>().map(|val| val.shortValue()));

        let service_data = if let Some(val) =
            adv_data.objectForKey(unsafe { CBAdvertisementDataServiceDataKey })
        {
            unsafe {
                if let Some(val) = val.downcast_ref::<NSDictionary>() {
                    let mut res = HashMap::with_capacity(val.count());
                    for k in val.allKeys() {
                        if let Some(key) = k.downcast_ref::<CBUUID>() {
                            if let Some(val) = val
                                .objectForKey_unchecked(&k)
                                .and_then(|val| val.downcast_ref::<NSData>())
                            {
                                res.insert(
                                    BluetoothUuid::from_be_slice(key.data().as_bytes_unchecked())
                                        .unwrap(),
                                    val.to_vec(),
                                );
                            }
                        }
                    }
                    res
                } else {
                    HashMap::new()
                }
            }
        } else {
            HashMap::new()
        };

        let service_uuids = adv_data
            .objectForKey(unsafe { CBAdvertisementDataServiceUUIDsKey })
            .into_iter()
            .flat_map(|x| x.downcast::<NSArray>())
            .flatten()
            .flat_map(|obj| obj.downcast::<CBUUID>())
            .map(|uuid| unsafe { uuid.data() })
            .map(|data| unsafe { BluetoothUuid::from_be_slice(data.as_bytes_unchecked()).unwrap() })
            .collect();

        let overflow_service_uuids = adv_data
            .objectForKey(unsafe { CBAdvertisementDataOverflowServiceUUIDsKey })
            .into_iter()
            .flat_map(|x| x.downcast::<NSArray>())
            .flatten()
            .flat_map(|obj| obj.downcast::<CBUUID>())
            .map(|uuid| unsafe { uuid.data() })
            .map(|data| unsafe { BluetoothUuid::from_be_slice(data.as_bytes_unchecked()).unwrap() })
            .collect();

        let solicited_service_uuids = adv_data
            .objectForKey(unsafe { CBAdvertisementDataSolicitedServiceUUIDsKey })
            .into_iter()
            .flat_map(|x| x.downcast::<NSArray>())
            .flatten()
            .flat_map(|obj| obj.downcast::<CBUUID>())
            .map(|uuid| unsafe { uuid.data() })
            .map(|data| unsafe { BluetoothUuid::from_be_slice(data.as_bytes_unchecked()).unwrap() })
            .collect();

        AdvertisementData {
            local_name,
            manufacturer_data,
            service_data,
            service_uuids,
            overflow_service_uuids,
            tx_power_level,
            is_connectable,
            solicited_service_uuids,
        }
    }
}
