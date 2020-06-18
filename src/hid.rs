use std::ffi::CStr;
use std::mem::{size_of, size_of_val, MaybeUninit};
use std::ptr::null_mut;

use winapi::{
    shared::{
        hidclass::GUID_DEVINTERFACE_HID,
        hidpi::{HidP_GetCaps, HIDP_STATUS_SUCCESS},
        hidsdi::{
            HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetManufacturerString,
            HidD_GetPreparsedData, HidD_GetProductString, HidD_GetSerialNumberString,
            HIDD_ATTRIBUTES,
        },
    },
    um::{
        fileapi::{CreateFileA, OPEN_EXISTING},
        handleapi::INVALID_HANDLE_VALUE,
        setupapi::{
            SetupDiEnumDeviceInfo, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsA,
            SetupDiGetDeviceInterfaceDetailA, SetupDiGetDeviceRegistryPropertyA,
            DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, SPDRP_CLASS, SPDRP_DRIVER,
            SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_A, SP_DEVINFO_DATA,
        },
        winbase::FILE_FLAG_OVERLAPPED,
        winnt::{FILE_SHARE_READ, FILE_SHARE_WRITE},
    },
};

use crate::types::{HidDeviceInfo, FALSE};

// TODO: Error handling
pub fn get_hid_device_info_list() -> Vec<HidDeviceInfo> {
    // Get information for all the devices belonging to the HID class
    let mut result = Vec::new();
    let mut index = 0;
    let device_info_set = unsafe {
        SetupDiGetClassDevsA(
            &mut GUID_DEVINTERFACE_HID,
            null_mut(),
            null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    };

    // Iterate over each device in the HID class
    loop {
        let mut device_interface_data = SP_DEVICE_INTERFACE_DATA::default();
        device_interface_data.cbSize = size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
        let res = unsafe {
            SetupDiEnumDeviceInterfaces(
                device_info_set,
                null_mut(),
                &mut GUID_DEVINTERFACE_HID,
                index,
                &mut device_interface_data,
            )
        };
        // Return of FALSE means there are no more devices.
        bool_check!(res);

        // let the function tell us how long the detail struct needs to be. The
        // size is put in required_size.
        let mut required_size: u32 = 0;
        unsafe {
            SetupDiGetDeviceInterfaceDetailA(
                device_info_set,
                &mut device_interface_data,
                null_mut(),
                0,
                &mut required_size,
                null_mut(),
            )
        };

        // Get the detailed data for this device. The detail data gives us
        // the device path for this device, which is then passed into
        // CreateFile() to get a handle to the device.
        // This has to be malloc here as it is a DST.
        // Box or other dynamic allocation method in std will fail.
        let mut device_interface_detail_data = unsafe {
            libc::malloc(required_size as usize) as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_A
        };
        unsafe {
            (*device_interface_detail_data).cbSize =
                size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_A>() as u32
        };
        let res = unsafe {
            SetupDiGetDeviceInterfaceDetailA(
                device_info_set,
                &mut device_interface_data,
                device_interface_detail_data,
                required_size,
                null_mut(),
                null_mut(),
            )
        };
        bool_check!(res);

        // Make sure this device is of Setup Class "HIDClass" and has a
        // driver bound to it.
        let mut devinfo_data = SP_DEVINFO_DATA::default();
        devinfo_data.cbSize = size_of::<SP_DEVINFO_DATA>() as u32;
        // Populate devinfo_data. This function will return failure
        // when there are no more interfaces left.
        let res = unsafe { SetupDiEnumDeviceInfo(device_info_set, index, &mut devinfo_data) };
        bool_check!(res);

        let mut driver_name = [0u8; 16];
        let res = unsafe {
            SetupDiGetDeviceRegistryPropertyA(
                device_info_set,
                &mut devinfo_data,
                SPDRP_CLASS,
                null_mut(),
                driver_name.as_mut_ptr(),
                256,
                null_mut(),
            )
        };
        bool_check!(res);

        let mut i = 0;
        driver_name.iter().for_each(|c| {
            if *c != 0 {
                i += 1;
            }
        });
        let driver = std::str::from_utf8(&driver_name[..i]).unwrap();
        match driver {
            "HIDClass" | "Mouse" | "Keyboard" => {
                let res = unsafe {
                    SetupDiGetDeviceRegistryPropertyA(
                        device_info_set,
                        &mut devinfo_data,
                        SPDRP_DRIVER,
                        null_mut(),
                        driver_name.as_mut_ptr(),
                        256,
                        null_mut(),
                    )
                };
                bool_check!(res);
            }
            _ => break,
        }

        // // Open a handle to the device
        let write_handle = unsafe {
            CreateFileA(
                (*device_interface_detail_data).DevicePath.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                null_mut(),
            )
        };
        if write_handle == INVALID_HANDLE_VALUE {
            break;
        }

        // Get the Vendor ID and Product ID for this device.
        let mut attrib = HIDD_ATTRIBUTES::default();
        attrib.Size = size_of::<HIDD_ATTRIBUTES>() as u32;
        unsafe { HidD_GetAttributes(write_handle, &mut attrib) };

        // Check the VID/PID to see if we should add this
        // device to the enumeration list.
        let mut cur_dev = HidDeviceInfo::default();

        // Get the Usage Page and Usage for this device
        let mut pp_data = MaybeUninit::uninit();
        let mut caps = MaybeUninit::uninit();
        let res = unsafe { HidD_GetPreparsedData(write_handle, pp_data.as_mut_ptr()) };
        if res != 0 {
            let pp_data = unsafe { pp_data.assume_init() };
            let nt_res = unsafe { HidP_GetCaps(pp_data, caps.as_mut_ptr()) };
            if nt_res == HIDP_STATUS_SUCCESS {
                let caps = unsafe { caps.assume_init() };
                cur_dev.usage_page = caps.UsagePage;
                cur_dev.usage = caps.Usage;
            }
            unsafe { HidD_FreePreparsedData(pp_data) };
        }

        // Path
        let cstr = unsafe {
            CStr::from_ptr((*device_interface_detail_data).DevicePath.as_ptr())
                .to_str()
                .unwrap()
        };
        cur_dev.path = String::from(cstr);

        // VID/PID/Release Number
        cur_dev.vendor_id = attrib.VendorID;
        cur_dev.product_id = attrib.ProductID;
        cur_dev.release_number = attrib.VersionNumber;

        // Serial Number
        let mut wstr = [0u16; 512];
        let res = unsafe {
            HidD_GetSerialNumberString(
                write_handle,
                wstr.as_mut_ptr() as *mut _,
                size_of_val(&wstr) as u32,
            )
        };
        if res != 0 {
            let mut i = 0;
            wstr.iter().for_each(|c| {
                if *c != 0 {
                    i += 1;
                }
            });
            cur_dev.serial_number = String::from_utf16(&wstr[..i]).unwrap();
        }

        // Manufacturer String
        let mut wstr = [0u16; 512];
        let res = unsafe {
            HidD_GetManufacturerString(
                write_handle,
                wstr.as_mut_ptr() as *mut _,
                size_of_val(&wstr) as u32,
            )
        };
        if res != 0 {
            let mut i = 0;
            wstr.iter().for_each(|c| {
                if *c != 0 {
                    i += 1;
                }
            });
            cur_dev.manufacturer_string = String::from_utf16(&wstr[..i]).unwrap();
        }

        // Product String
        let mut wstr = [0u16; 512];
        let res = unsafe {
            HidD_GetProductString(
                write_handle,
                wstr.as_mut_ptr() as *mut _,
                size_of_val(&wstr) as u32,
            )
        };
        if res != 0 {
            let mut i = 0;
            wstr.iter().for_each(|c| {
                if *c != 0 {
                    i += 1;
                }
            });
            cur_dev.product_string = String::from_utf16(&wstr[..i]).unwrap();
        }

        // Interface Number. It can sometimes be parsed out of the path
        // on Windows if a device has multiple interfaces. See
        // http://msdn.microsoft.com/en-us/windows/hardware/gg487473 or
        // search for "Hardware IDs for HID Devices" at MSDN. If it's not
        // in the path, it's set to -1.
        cur_dev.interface_number = if let Some(i) = cur_dev.path.find("&mi_") {
            i32::from_str_radix(&cur_dev.path[i + 4..i + 6], 16).unwrap()
        } else {
            -1
        };

        result.push(cur_dev);
        index += 1;
    }

    result
}
