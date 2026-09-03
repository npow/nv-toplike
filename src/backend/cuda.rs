// SPDX-License-Identifier: Apache-2.0

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::ffi::c_char;

use libloading::{Library, Symbol};

const CU_DEVICE_ATTRIBUTE_MULTIPROCESSOR_COUNT: i32 = 16;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CuUuid {
    bytes: [u8; 16],
}

type CuInitFn = unsafe extern "C" fn(flags: u32) -> i32;
type CuDeviceGetCountFn = unsafe extern "C" fn(count: *mut i32) -> i32;
type CuDeviceGetFn = unsafe extern "C" fn(device: *mut i32, ordinal: i32) -> i32;
type CuDeviceGetUuidFn = unsafe extern "C" fn(uuid: *mut CuUuid, dev: i32) -> i32;
type CuDeviceGetAttributeFn = unsafe extern "C" fn(pi: *mut i32, attrib: i32, dev: i32) -> i32;
type CuDeviceGetPciBusIdFn =
    unsafe extern "C" fn(pci_bus_id: *mut c_char, len: i32, dev: i32) -> i32;

#[derive(Debug, Default, Clone)]
pub struct CudaDeviceTable {
    by_uuid: HashMap<String, u32>,
    by_pci: HashMap<String, u32>,
    by_index: HashMap<u32, u32>,
}

impl CudaDeviceTable {
    /// Attempt to query SM counts for all visible CUDA physical devices
    /// via the NVIDIA CUDA driver library (`libcuda.so.1` / `nvcuda.dll`).
    ///
    /// If the CUDA driver library is not available, fails to initialize,
    /// or encounters an error, returns an empty table.
    pub fn query() -> Self {
        Self::try_query().unwrap_or_default()
    }

    fn try_query() -> Result<Self, Box<dyn std::error::Error>> {
        #[cfg(target_os = "windows")]
        let lib_candidates = ["nvcuda.dll"];
        #[cfg(not(target_os = "windows"))]
        let lib_candidates = ["libcuda.so.1", "libcuda.so"];

        let mut lib = None;
        for candidate in lib_candidates {
            if let Ok(loaded) = unsafe { Library::new(candidate) } {
                lib = Some(loaded);
                break;
            }
        }

        let lib = match lib {
            Some(l) => l,
            None => return Ok(Self::default()),
        };

        unsafe {
            let cu_init: Symbol<CuInitFn> = lib.get(b"cuInit")?;
            if cu_init(0) != 0 {
                return Ok(Self::default());
            }

            let cu_device_get_count: Symbol<CuDeviceGetCountFn> = lib.get(b"cuDeviceGetCount")?;
            let cu_device_get: Symbol<CuDeviceGetFn> = lib.get(b"cuDeviceGet")?;
            let cu_device_get_attribute: Symbol<CuDeviceGetAttributeFn> =
                lib.get(b"cuDeviceGetAttribute")?;

            let cu_device_get_uuid: Option<Symbol<CuDeviceGetUuidFn>> = lib
                .get(b"cuDeviceGetUuid_v2")
                .or_else(|_| lib.get(b"cuDeviceGetUuid"))
                .ok();

            let cu_device_get_pci_bus_id: Option<Symbol<CuDeviceGetPciBusIdFn>> =
                lib.get(b"cuDeviceGetPCIBusId").ok();

            let mut count: i32 = 0;
            if cu_device_get_count(&mut count) != 0 || count <= 0 {
                return Ok(Self::default());
            }

            let mut table = Self::default();

            for ordinal in 0..count {
                let mut dev: i32 = 0;
                if cu_device_get(&mut dev, ordinal) != 0 {
                    continue;
                }

                let mut sm_count: i32 = 0;
                if cu_device_get_attribute(
                    &mut sm_count,
                    CU_DEVICE_ATTRIBUTE_MULTIPROCESSOR_COUNT,
                    dev,
                ) != 0
                    || sm_count <= 0
                {
                    continue;
                }

                let sm_count_u32 = sm_count as u32;
                table.by_index.insert(ordinal as u32, sm_count_u32);

                if let Some(ref get_uuid) = cu_device_get_uuid {
                    let mut uuid = CuUuid::default();
                    if get_uuid(&mut uuid, dev) == 0 {
                        let formatted = format_uuid(&uuid.bytes);
                        table
                            .by_uuid
                            .insert(normalize_uuid(&formatted), sm_count_u32);
                    }
                }

                if let Some(ref get_pci) = cu_device_get_pci_bus_id {
                    let mut pci_buf = [0 as c_char; 64];
                    if get_pci(pci_buf.as_mut_ptr(), 64, dev) == 0 {
                        let c_str = std::ffi::CStr::from_ptr(pci_buf.as_ptr());
                        if let Ok(pci_str) = c_str.to_str() {
                            table.by_pci.insert(normalize_pci(pci_str), sm_count_u32);
                        }
                    }
                }
            }

            Ok(table)
        }
    }

    /// Retrieve the SM count for a device by checking UUID, PCI Bus ID, or display index.
    pub fn get_sm_count(
        &self,
        uuid: &str,
        pci_bus_id: Option<&str>,
        display_index: Option<u32>,
    ) -> Option<u32> {
        let norm_uuid = normalize_uuid(uuid);
        if let Some(&count) = self.by_uuid.get(&norm_uuid) {
            return Some(count);
        }
        if let Some(pci) = pci_bus_id {
            let norm_pci = normalize_pci(pci);
            if let Some(&count) = self.by_pci.get(&norm_pci) {
                return Some(count);
            }
        }
        if let Some(index) = display_index
            && let Some(&count) = self.by_index.get(&index)
        {
            return Some(count);
        }
        None
    }
}

fn format_uuid(b: &[u8; 16]) -> String {
    format!(
        "GPU-{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0],
        b[1],
        b[2],
        b[3],
        b[4],
        b[5],
        b[6],
        b[7],
        b[8],
        b[9],
        b[10],
        b[11],
        b[12],
        b[13],
        b[14],
        b[15]
    )
}

fn normalize_uuid(uuid: &str) -> String {
    let lower = uuid.trim().to_ascii_lowercase();
    lower.strip_prefix("gpu-").unwrap_or(&lower).to_owned()
}

fn normalize_pci(pci: &str) -> String {
    pci.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_formatting_and_normalization() {
        let raw: [u8; 16] = [
            0x8a, 0xb9, 0x50, 0x02, 0x47, 0x2c, 0x25, 0xfd, 0x9c, 0xd8, 0xc4, 0x2c, 0x48, 0x13,
            0x09, 0xd6,
        ];
        let formatted = format_uuid(&raw);
        assert_eq!(formatted, "GPU-8ab95002-472c-25fd-9cd8-c42c481309d6");
        assert_eq!(
            normalize_uuid(&formatted),
            "8ab95002-472c-25fd-9cd8-c42c481309d6"
        );
        assert_eq!(
            normalize_uuid("GPU-8AB95002-472C-25FD-9CD8-C42C481309D6"),
            "8ab95002-472c-25fd-9cd8-c42c481309d6"
        );
    }

    #[test]
    fn table_lookup_fallbacks() {
        let mut table = CudaDeviceTable::default();
        table
            .by_uuid
            .insert("8ab95002-472c-25fd-9cd8-c42c481309d6".to_owned(), 188);
        table.by_pci.insert("00000000:61:00.0".to_owned(), 188);
        table.by_index.insert(0, 188);

        assert_eq!(
            table.get_sm_count("GPU-8ab95002-472c-25fd-9cd8-c42c481309d6", None, None),
            Some(188)
        );
        assert_eq!(
            table.get_sm_count("GPU-8AB95002-472C-25FD-9CD8-C42C481309D6", None, None),
            Some(188)
        );
        assert_eq!(
            table.get_sm_count("unknown", Some("00000000:61:00.0"), None),
            Some(188)
        );
        assert_eq!(table.get_sm_count("unknown", None, Some(0)), Some(188));
        assert_eq!(table.get_sm_count("unknown", None, Some(1)), None);
    }
}
