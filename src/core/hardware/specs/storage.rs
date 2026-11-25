//! Discos físicos por `IOCTL_STORAGE_QUERY_PROPERTY`.

use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_ID,
    STORAGE_PROPERTY_QUERY, STORAGE_QUERY_TYPE,
};

use super::StorageSpecs;
use crate::core::hardware::drives::{
    MAX_PHYSICAL_DRIVES, bus_label, drive_model, open_physical_drive,
};
use crate::core::platform::memory::AlignedBuf;
use crate::core::platform::wide::read_cstr_at;

pub(crate) fn read_storage_devices() -> Vec<StorageSpecs> {
    let mut drives = Vec::new();

    for idx in 0..MAX_PHYSICAL_DRIVES {
        let Some(drive) = open_physical_drive(idx, 0) else {
            continue;
        };
        let raw_handle = drive.raw();

        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: STORAGE_PROPERTY_ID(0),
            QueryType: STORAGE_QUERY_TYPE(0),
            AdditionalParameters: [0],
        };

        let mut out_buf = AlignedBuf::zeroed(1024);
        let mut bytes_returned = 0u32;

        let ok = unsafe {
            DeviceIoControl(
                raw_handle,
                IOCTL_STORAGE_QUERY_PROPERTY,
                Some((&query as *const STORAGE_PROPERTY_QUERY).cast()),
                std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                Some(out_buf.as_mut_ptr().cast()),
                out_buf.len() as u32,
                Some(&mut bytes_returned),
                None,
            )
            .is_ok()
        };

        if !ok || (bytes_returned as usize) < std::mem::size_of::<STORAGE_DEVICE_DESCRIPTOR>() {
            continue;
        }

        let desc = unsafe { &*(out_buf.as_ptr().cast::<STORAGE_DEVICE_DESCRIPTOR>()) };
        let valid_slice = &out_buf[..bytes_returned as usize];

        let vendor = read_cstr_at(valid_slice, desc.VendorIdOffset);
        let product = read_cstr_at(valid_slice, desc.ProductIdOffset);
        let firmware = read_cstr_at(valid_slice, desc.ProductRevisionOffset);
        let serial = read_cstr_at(valid_slice, desc.SerialNumberOffset);

        let Some(model) = drive_model(&vendor, &product) else {
            continue;
        };
        let bus_type = bus_label(desc.BusType.0).to_string();

        {
            drives.push(StorageSpecs {
                device_index: idx,
                model,
                firmware,
                bus_type,
                serial,
            });
        }
    }

    drives
}
