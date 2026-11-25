//! Temperatura de discos por `IOCTL_STORAGE_QUERY_PROPERTY` (StorageDeviceTemperatureProperty).

use std::mem::size_of;

use windows::Win32::Storage::FileSystem::FILE_GENERIC_READ;
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_PROPERTY_QUERY,
    STORAGE_TEMPERATURE_DATA_DESCRIPTOR, STORAGE_TEMPERATURE_INFO,
    StorageDeviceTemperatureProperty,
};

use super::{SensorReading, SensorSession};
use crate::core::hardware::drives::{MAX_PHYSICAL_DRIVES, open_physical_drive};
use crate::core::platform::memory::AlignedBuf;

impl SensorSession {
    pub fn poll_storage(&self) -> Vec<SensorReading> {
        let mut readings = Vec::new();

        for drive_num in 0..MAX_PHYSICAL_DRIVES {
            let Some(handle) = open_physical_drive(drive_num, FILE_GENERIC_READ.0) else {
                continue;
            };

            let query = STORAGE_PROPERTY_QUERY {
                PropertyId: StorageDeviceTemperatureProperty,
                QueryType: PropertyStandardQuery,
                AdditionalParameters: [0; 1],
            };

            let mut header = STORAGE_TEMPERATURE_DATA_DESCRIPTOR::default();
            let mut returned = 0u32;

            let ok = unsafe {
                DeviceIoControl(
                    handle.raw(),
                    IOCTL_STORAGE_QUERY_PROPERTY,
                    Some(&query as *const _ as *const _),
                    size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                    Some(&mut header as *mut _ as *mut _),
                    size_of::<STORAGE_TEMPERATURE_DATA_DESCRIPTOR>() as u32,
                    Some(&mut returned),
                    None,
                )
            };

            if ok.is_err()
                || (header.Size as usize) < size_of::<STORAGE_TEMPERATURE_DATA_DESCRIPTOR>()
            {
                continue;
            }

            let buf_size = (header.Size as usize).min(4096);
            let mut full_buf = AlignedBuf::zeroed(buf_size);
            let full_ok = unsafe {
                DeviceIoControl(
                    handle.raw(),
                    IOCTL_STORAGE_QUERY_PROPERTY,
                    Some(&query as *const _ as *const _),
                    size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                    Some(full_buf.as_mut_ptr() as *mut _),
                    buf_size as u32,
                    Some(&mut returned),
                    None,
                )
            };

            if full_ok.is_err()
                || (returned as usize) < size_of::<STORAGE_TEMPERATURE_DATA_DESCRIPTOR>()
            {
                continue;
            }

            let desc =
                unsafe { &*(full_buf.as_ptr() as *const STORAGE_TEMPERATURE_DATA_DESCRIPTOR) };

            let info_offset = size_of::<STORAGE_TEMPERATURE_DATA_DESCRIPTOR>()
                - size_of::<STORAGE_TEMPERATURE_INFO>();
            let max_possible = (returned as usize).saturating_sub(info_offset)
                / size_of::<STORAGE_TEMPERATURE_INFO>();
            let count = (desc.InfoCount as usize).min(max_possible);

            if count == 0 {
                continue;
            }

            let infos = unsafe { std::slice::from_raw_parts(desc.TemperatureInfo.as_ptr(), count) };

            let crit = if desc.CriticalTemperature > 0 && desc.CriticalTemperature < 150 {
                Some(desc.CriticalTemperature as f32)
            } else {
                None
            };

            for info in infos {
                if info.Temperature > 0 && info.Temperature < 125 {
                    let label = if count > 1 {
                        format!("Disco {} (Sensor {})", drive_num, info.Index)
                    } else {
                        format!("Disco {}", drive_num)
                    };
                    let over = (info.OverThreshold > 0 && info.OverThreshold < 150)
                        .then_some(info.OverThreshold as f32);
                    let max_c = crit.or(over);

                    readings.push(SensorReading {
                        name: label,
                        sensor_type: "NVMe / SSD Storage".into(),
                        current_celsius: info.Temperature as f32,
                        max_celsius: max_c,
                    });
                }
            }
        }

        readings
    }
}
