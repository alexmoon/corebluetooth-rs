use btuuid::BluetoothUuid;
use objc2::rc::Retained;
use objc2_core_bluetooth::CBUUID;
use objc2_foundation::NSData;

pub fn to_cbuuid(uuid: &BluetoothUuid) -> Retained<CBUUID> {
    let data = match uuid {
        BluetoothUuid::Uuid16(uuid) => NSData::with_bytes(&uuid.to_be_bytes()),
        BluetoothUuid::Uuid32(uuid) => NSData::with_bytes(&uuid.to_be_bytes()),
        BluetoothUuid::Uuid128(uuid) => NSData::with_bytes(&uuid.to_be_bytes()),
    };
    unsafe { CBUUID::UUIDWithData(&data) }
}
