//! Discos físicos por `IOCTL_STORAGE_QUERY_PROPERTY`.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_ID,
    STORAGE_PROPERTY_QUERY, STORAGE_QUERY_TYPE,
};

use super::StorageSpecs;
use crate::core::platform::handles::SafeHandle;
use crate::core::platform::memory::AlignedBuf;
use crate::core::platform::wide::read_cstr_at;

pub(super) fn read_storage_devices() -> Vec<StorageSpecs> {
    let mut drives = Vec::new();

    for idx in 0..8u32 {
        let path_str = format!(r"\\.\PhysicalDrive{idx}");
        let wide: Vec<u16> = OsStr::new(&path_str)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let handle = unsafe {
            CreateFileW(
                windows::core::PCWSTR(wide.as_ptr()),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                HANDLE::default(),
            )
        };

        let Some(drive) = handle.ok().and_then(SafeHandle::new) else {
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

        let model = if !vendor.is_empty() && !product.starts_with(&vendor) {
            format!("{vendor} {product}")
        } else {
            product
        };

        let bus_type = match desc.BusType.0 {
            17 => "NVMe",
            11 => "SATA",
            7 => "USB",
            8 => "RAID",
            _ => "Almacenamiento",
        }
        .to_string();

        if !model.is_empty() {
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
