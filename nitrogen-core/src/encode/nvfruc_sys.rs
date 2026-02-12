//! Raw FFI bindings for NVIDIA Optical Flow FRUC (Frame Rate Up Conversion) library
//!
//! These bindings are loaded dynamically at runtime from libNvOFFRUC.so.
//! The library is part of NVIDIA's Optical Flow SDK and provides
//! hardware-accelerated frame interpolation on Turing+ GPUs.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use std::ffi::c_void;

/// Maximum number of resources NvOFFRUC can register
pub const NVOFFRUC_MAX_RESOURCE: usize = 10;

/// Minimum number of resources required before NvOFFRUCProcess can be called
pub const NVOFFRUC_MIN_RESOURCE: usize = 3;

/// FRUC API return status codes
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvOFFRUC_STATUS {
    SUCCESS = 0,
    ERR_NOT_SUPPORTED = 1,
    ERR_INVALID_PTR = 2,
    ERR_INVALID_PARAM = 3,
    ERR_INVALID_HANDLE = 4,
    ERR_OUT_OF_SYSTEM_MEMORY = 5,
    ERR_OUT_OF_VIDEO_MEMORY = 6,
    ERR_OPENCV_NOT_AVAILABLE = 7,
    ERR_UNIMPLEMENTED = 8,
    ERR_OF_FAILURE = 9,
    ERR_DUPLICATE_RESOURCE = 10,
    ERR_UNREGISTERED_RESOURCE = 11,
    ERR_INCORRECT_API_SEQUENCE = 12,
    ERR_WRITE_TODISK_FAILED = 13,
    ERR_PIPELINE_EXECUTION_FAILURE = 14,
    ERR_SYNC_WRITE_FAILED = 15,
    ERR_GENERIC = 16,
}

/// CUDA resource type
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvOFFRUCCUDAResourceType {
    Undefined = -1,
    CuDevicePtr = 0,
    CuArray = 1,
}

/// Resource type for FRUC operations
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvOFFRUCResourceType {
    Undefined = -1,
    CudaResource = 0,
    DirectX11Resource = 1,
}

/// Surface format for input/output frames
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvOFFRUCSurfaceFormat {
    Undefined = -1,
    NV12 = 0,
    ARGB = 1,
}

/// Parameters for creating a FRUC instance
#[repr(C)]
#[derive(Debug, Clone)]
pub struct NvOFFRUC_CREATE_PARAM {
    /// Width of input/output video surface
    pub uiWidth: u32,
    /// Height of input/output video surface
    pub uiHeight: u32,
    /// D3D device created by client (NULL for CUDA)
    pub pDevice: *mut c_void,
    /// Resource type (CUDA or DirectX)
    pub eResourceType: NvOFFRUCResourceType,
    /// Surface format (NV12 or ARGB)
    pub eSurfaceFormat: NvOFFRUCSurfaceFormat,
    /// CUDA resource type (cuDevicePtr or cuArray)
    pub eCUDAResourceType: NvOFFRUCCUDAResourceType,
    /// Reserved - must be zeroed
    pub uiReserved: [u32; 32],
}

impl Default for NvOFFRUC_CREATE_PARAM {
    fn default() -> Self {
        Self {
            uiWidth: 0,
            uiHeight: 0,
            pDevice: std::ptr::null_mut(),
            eResourceType: NvOFFRUCResourceType::CudaResource,
            eSurfaceFormat: NvOFFRUCSurfaceFormat::ARGB,
            eCUDAResourceType: NvOFFRUCCUDAResourceType::CuDevicePtr,
            uiReserved: [0; 32],
        }
    }
}

/// Frame data structure for input/output
#[repr(C)]
#[derive(Debug, Clone)]
pub struct NvOFFRUC_FRAMEDATA {
    /// Frame pointer (D3D texture or CUDA ptr)
    pub pFrame: *mut c_void,
    /// Frame timestamp
    pub nTimeStamp: f64,
    /// Pitch for CUDA pitch allocations
    pub nCuSurfacePitch: usize,
    /// Output flag indicating frame repetition occurred
    pub bHasFrameRepetitionOccurred: *mut bool,
    /// Reserved
    pub uiReserved: [u32; 32],
}

impl Default for NvOFFRUC_FRAMEDATA {
    fn default() -> Self {
        Self {
            pFrame: std::ptr::null_mut(),
            nTimeStamp: 0.0,
            nCuSurfacePitch: 0,
            bHasFrameRepetitionOccurred: std::ptr::null_mut(),
            uiReserved: [0; 32],
        }
    }
}

/// Sync wait union (for fence/mutex synchronization)
#[repr(C)]
#[derive(Clone, Copy)]
pub union SyncWait {
    pub fence_wait_value: u64,
    pub mutex_acquire_keys: [u64; 2],
}

impl Default for SyncWait {
    fn default() -> Self {
        Self { fence_wait_value: 0 }
    }
}

/// Sync signal union (for fence/mutex synchronization)
#[repr(C)]
#[derive(Clone, Copy)]
pub union SyncSignal {
    pub fence_signal_value: u64,
    pub mutex_release_keys: [u64; 2],
}

impl Default for SyncSignal {
    fn default() -> Self {
        Self { fence_signal_value: 0 }
    }
}

/// Input parameters for NvOFFRUCProcess
#[repr(C)]
pub struct NvOFFRUC_PROCESS_IN_PARAMS {
    /// Input frame data
    pub stFrameDataInput: NvOFFRUC_FRAMEDATA,
    /// Skip warping flag (bit 0)
    pub bSkipWarp: u32,
    /// Sync wait data
    pub uSyncWait: SyncWait,
    /// Reserved
    pub uiReserved: [u32; 32],
}

impl Default for NvOFFRUC_PROCESS_IN_PARAMS {
    fn default() -> Self {
        Self {
            stFrameDataInput: NvOFFRUC_FRAMEDATA::default(),
            bSkipWarp: 0,
            uSyncWait: SyncWait::default(),
            uiReserved: [0; 32],
        }
    }
}

/// Output parameters for NvOFFRUCProcess
#[repr(C)]
pub struct NvOFFRUC_PROCESS_OUT_PARAMS {
    /// Output frame data
    pub stFrameDataOutput: NvOFFRUC_FRAMEDATA,
    /// Sync signal data
    pub uSyncSignal: SyncSignal,
    /// Reserved
    pub uiReserved: [u32; 32],
}

impl Default for NvOFFRUC_PROCESS_OUT_PARAMS {
    fn default() -> Self {
        Self {
            stFrameDataOutput: NvOFFRUC_FRAMEDATA::default(),
            uSyncSignal: SyncSignal::default(),
            uiReserved: [0; 32],
        }
    }
}

/// Resource registration parameters
#[repr(C)]
pub struct NvOFFRUC_REGISTER_RESOURCE_PARAM {
    /// Array of resources to register
    pub pArrResource: [*mut c_void; NVOFFRUC_MAX_RESOURCE],
    /// D3D11 fence object (NULL for CUDA)
    pub pD3D11FenceObj: *mut c_void,
    /// Count of resources in array
    pub uiCount: u32,
}

impl Default for NvOFFRUC_REGISTER_RESOURCE_PARAM {
    fn default() -> Self {
        Self {
            pArrResource: [std::ptr::null_mut(); NVOFFRUC_MAX_RESOURCE],
            pD3D11FenceObj: std::ptr::null_mut(),
            uiCount: 0,
        }
    }
}

/// Resource unregistration parameters
#[repr(C)]
pub struct NvOFFRUC_UNREGISTER_RESOURCE_PARAM {
    /// Array of resources to unregister
    pub pArrResource: [*mut c_void; NVOFFRUC_MAX_RESOURCE],
    /// Count of resources in array
    pub uiCount: u32,
}

impl Default for NvOFFRUC_UNREGISTER_RESOURCE_PARAM {
    fn default() -> Self {
        Self {
            pArrResource: [std::ptr::null_mut(); NVOFFRUC_MAX_RESOURCE],
            uiCount: 0,
        }
    }
}

/// Opaque handle to FRUC instance
pub type NvOFFRUCHandle = *mut c_void;

/// Function pointer types for dynamically loaded functions
pub type FnNvOFFRUCCreate = unsafe extern "C" fn(
    pCreateParam: *const NvOFFRUC_CREATE_PARAM,
    phFRUC: *mut NvOFFRUCHandle,
) -> NvOFFRUC_STATUS;

pub type FnNvOFFRUCRegisterResource = unsafe extern "C" fn(
    hFRUC: NvOFFRUCHandle,
    pRegisterParam: *const NvOFFRUC_REGISTER_RESOURCE_PARAM,
) -> NvOFFRUC_STATUS;

pub type FnNvOFFRUCUnregisterResource = unsafe extern "C" fn(
    hFRUC: NvOFFRUCHandle,
    pUnregisterParam: *const NvOFFRUC_UNREGISTER_RESOURCE_PARAM,
) -> NvOFFRUC_STATUS;

pub type FnNvOFFRUCProcess = unsafe extern "C" fn(
    hFRUC: NvOFFRUCHandle,
    pInParams: *const NvOFFRUC_PROCESS_IN_PARAMS,
    pOutParams: *const NvOFFRUC_PROCESS_OUT_PARAMS,
) -> NvOFFRUC_STATUS;

pub type FnNvOFFRUCDestroy = unsafe extern "C" fn(hFRUC: NvOFFRUCHandle) -> NvOFFRUC_STATUS;

/// Library paths to search for libNvOFFRUC.so (NVIDIA Optical Flow FRUC)
pub const NVFRUC_LIB_PATHS: &[&str] = &[
    // Standard naming from NVIDIA SDK
    "libNvOFFRUC.so",
    "/usr/local/lib/libNvOFFRUC.so",
    "/usr/lib/libNvOFFRUC.so",
    "/usr/lib/x86_64-linux-gnu/libNvOFFRUC.so",
    "/usr/lib64/libNvOFFRUC.so",
    "/opt/nvidia/lib/libNvOFFRUC.so",
];

/// Dynamically loaded FRUC library
pub struct NvOFFRUCLib {
    _lib: libloading::Library,
    pub create: FnNvOFFRUCCreate,
    pub register_resource: FnNvOFFRUCRegisterResource,
    pub unregister_resource: FnNvOFFRUCUnregisterResource,
    pub process: FnNvOFFRUCProcess,
    pub destroy: FnNvOFFRUCDestroy,
}

impl NvOFFRUCLib {
    /// Try to load the NvOFFRUC library from standard paths
    pub fn load() -> Result<Self, String> {
        for path in NVFRUC_LIB_PATHS {
            if let Ok(lib) = Self::load_from_path(path) {
                tracing::info!("Loaded NvOFFRUC library from: {}", path);
                return Ok(lib);
            }
        }
        Err("Failed to load libNvOFFRUC.so from any known path".to_string())
    }

    /// Load the library from a specific path
    ///
    /// # Safety
    /// This function uses unsafe to:
    /// 1. Load a dynamic library which could execute arbitrary code in its init
    /// 2. Look up function symbols and cast them to Rust function pointers
    ///
    /// We mitigate risks by:
    /// - Only loading from known system paths (NVFRUC_LIB_PATHS)
    /// - Verifying all required symbols exist before returning success
    /// - Using the correct function signatures as defined by NVIDIA's SDK
    pub fn load_from_path(path: &str) -> Result<Self, String> {
        // SAFETY: We are loading the NVIDIA FRUC library which is a trusted system library.
        // The function signatures match NVIDIA's documented API.
        // We copy the function pointers immediately so they remain valid after symbol lookup.
        unsafe {
            let lib = libloading::Library::new(path)
                .map_err(|e| format!("Failed to load {}: {}", path, e))?;

            // Get symbols and copy function pointers immediately
            let create_fn: FnNvOFFRUCCreate = *lib
                .get::<FnNvOFFRUCCreate>(b"NvOFFRUCCreate")
                .map_err(|e| format!("Failed to get NvOFFRUCCreate: {}", e))?;

            let register_resource_fn: FnNvOFFRUCRegisterResource = *lib
                .get::<FnNvOFFRUCRegisterResource>(b"NvOFFRUCRegisterResource")
                .map_err(|e| format!("Failed to get NvOFFRUCRegisterResource: {}", e))?;

            let unregister_resource_fn: FnNvOFFRUCUnregisterResource = *lib
                .get::<FnNvOFFRUCUnregisterResource>(b"NvOFFRUCUnregisterResource")
                .map_err(|e| format!("Failed to get NvOFFRUCUnregisterResource: {}", e))?;

            let process_fn: FnNvOFFRUCProcess = *lib
                .get::<FnNvOFFRUCProcess>(b"NvOFFRUCProcess")
                .map_err(|e| format!("Failed to get NvOFFRUCProcess: {}", e))?;

            let destroy_fn: FnNvOFFRUCDestroy = *lib
                .get::<FnNvOFFRUCDestroy>(b"NvOFFRUCDestroy")
                .map_err(|e| format!("Failed to get NvOFFRUCDestroy: {}", e))?;

            Ok(Self {
                _lib: lib,
                create: create_fn,
                register_resource: register_resource_fn,
                unregister_resource: unregister_resource_fn,
                process: process_fn,
                destroy: destroy_fn,
            })
        }
    }
}

// SAFETY: NvOFFRUCLib holds function pointers to the NVIDIA Optical Flow FRUC library.
// According to NVIDIA's documentation, these functions are thread-safe.
// The library handle (_lib) is kept alive for the lifetime of this struct,
// ensuring the function pointers remain valid.
unsafe impl Send for NvOFFRUCLib {}
unsafe impl Sync for NvOFFRUCLib {}

impl NvOFFRUC_STATUS {
    /// Check if status indicates success
    pub fn is_success(&self) -> bool {
        *self == NvOFFRUC_STATUS::SUCCESS
    }

    /// Convert to a human-readable error message
    pub fn to_error_string(&self) -> &'static str {
        match self {
            NvOFFRUC_STATUS::SUCCESS => "Success",
            NvOFFRUC_STATUS::ERR_NOT_SUPPORTED => "Optical flow not supported on this hardware",
            NvOFFRUC_STATUS::ERR_INVALID_PTR => "Invalid pointer",
            NvOFFRUC_STATUS::ERR_INVALID_PARAM => "Invalid parameter",
            NvOFFRUC_STATUS::ERR_INVALID_HANDLE => "Invalid handle",
            NvOFFRUC_STATUS::ERR_OUT_OF_SYSTEM_MEMORY => "Out of system memory",
            NvOFFRUC_STATUS::ERR_OUT_OF_VIDEO_MEMORY => "Out of video memory",
            NvOFFRUC_STATUS::ERR_OPENCV_NOT_AVAILABLE => "OpenCV not available",
            NvOFFRUC_STATUS::ERR_UNIMPLEMENTED => "Feature not implemented",
            NvOFFRUC_STATUS::ERR_OF_FAILURE => "Optical flow failure",
            NvOFFRUC_STATUS::ERR_DUPLICATE_RESOURCE => "Resource already registered",
            NvOFFRUC_STATUS::ERR_UNREGISTERED_RESOURCE => "Resource not registered",
            NvOFFRUC_STATUS::ERR_INCORRECT_API_SEQUENCE => "Incorrect API call sequence",
            NvOFFRUC_STATUS::ERR_WRITE_TODISK_FAILED => "Failed to write to disk",
            NvOFFRUC_STATUS::ERR_PIPELINE_EXECUTION_FAILURE => "Pipeline execution failed",
            NvOFFRUC_STATUS::ERR_SYNC_WRITE_FAILED => "Sync write failed",
            NvOFFRUC_STATUS::ERR_GENERIC => "Generic error",
        }
    }
}
