use std::ffi::CStr;
use std::fmt;
use std::result::Result as StdResult;

use foreign_types::{foreign_type, ForeignType};
use thiserror::Error;

use crate::ffi::chimera as ffi;

/// A type for errors returned by Chimera functions.
#[derive(Debug, Error, PartialEq)]
pub enum Error {
    /// A parameter passed to this function was invalid.
    #[error("A parameter passed to this function was invalid.")]
    Invalid,

    /// A memory allocation failed.
    #[error("A memory allocation failed.")]
    NoMem,

    /// The engine was terminated by callback.
    ///
    /// This return value indicates that the target buffer was partially scanned,
    /// but that the callback function requested that scanning cease after a match
    /// was located.
    #[error("The engine was terminated by callback.")]
    ScanTerminated,

    /// The pattern compiler failed, and the `ch_compile_error_t` should be inspected for more detail.
    #[error("The pattern compiler failed with more detail, {0}.")]
    CompileError(CompileError),

    /// The pattern compiler failed.
    #[error("he pattern compiler failed.")]
    CompilerError,

    /// The given database was built for a different version of the Chimera matcher.
    #[error("he pattern compiler failed.")]
    DbVersionError,

    /// The given database was built for a different platform (i.e., CPU type).
    #[error("The given database was built for a different platform (i.e., CPU type).")]
    DbPlatformError,

    /// The given database was built for a different mode of operation.
    ///
    /// This error is returned when streaming calls are used with a non-streaming database and vice versa.
    #[error("The given database was built for a different mode of operation.")]
    DbModeError,

    /// A parameter passed to this function was not correctly aligned.
    #[error("A parameter passed to this function was not correctly aligned.")]
    BadAlign,

    /// The memory allocator did not correctly return memory suitably aligned for
    /// the largest representable data type on this platform.
    #[error("The memory allocator did not correctly return memory suitably aligned.")]
    BadAlloc,

    /// The scratch region was already in use.
    ///
    /// This error is returned when Chimera is able to detect that the scratch
    /// region given is already in use by another Chimera API call.
    ///
    /// A separate scratch region, allocated with @ref ch_alloc_scratch() or @ref
    /// ch_clone_scratch(), is required for every concurrent caller of the Chimera
    /// API.
    ///
    /// For example, this error might be returned when @ref ch_scan() has been
    /// called inside a callback delivered by a currently-executing @ref ch_scan()
    /// call using the same scratch region.
    ///
    /// Note: Not all concurrent uses of scratch regions may be detected. This error
    /// is intended as a best-effort debugging tool, not a guarantee.
    #[error("The scratch region was already in use.")]
    ScratchInUse,

    /// Returned when pcre_exec (called for some expressions internally from `ch_scan`) failed due to a fatal error.
    #[error("Failed due to a fatal error")]
    FailInternal,

    /// Unknown error code
    #[error("Unknown error code: {0}")]
    Code(ffi::ch_error_t),
}

impl From<ffi::ch_error_t> for Error {
    fn from(err: ffi::ch_error_t) -> Self {
        use Error::*;

        match err {
            ffi::CH_INVALID => Invalid,
            ffi::CH_NOMEM => NoMem,
            ffi::CH_SCAN_TERMINATED => ScanTerminated,
            // ffi::CH_COMPILER_ERROR => HsError::CompileError,
            ffi::CH_DB_VERSION_ERROR => DbVersionError,
            ffi::CH_DB_PLATFORM_ERROR => DbPlatformError,
            ffi::CH_DB_MODE_ERROR => DbModeError,
            ffi::CH_BAD_ALIGN => BadAlign,
            ffi::CH_BAD_ALLOC => BadAlloc,
            ffi::CH_SCRATCH_IN_USE => ScratchInUse,
            ffi::CH_FAIL_INTERNAL => FailInternal,
            _ => Code(err),
        }
    }
}

pub trait AsResult
where
    Self: Sized,
{
    type Output;
    type Error: fmt::Debug;

    fn ok(self) -> StdResult<Self::Output, Self::Error>;

    fn map<U, F: FnOnce(Self::Output) -> U>(self, op: F) -> StdResult<U, Self::Error> {
        self.ok().map(op)
    }

    fn and_then<U, F: FnOnce(Self::Output) -> StdResult<U, Self::Error>>(self, op: F) -> StdResult<U, Self::Error> {
        self.ok().and_then(op)
    }

    fn expect(self, msg: &str) -> Self::Output {
        self.ok().expect(msg)
    }
}

impl AsResult for ffi::ch_error_t {
    type Output = ();
    type Error = anyhow::Error;

    fn ok(self) -> StdResult<Self::Output, Self::Error> {
        if self == ffi::CH_SUCCESS as ffi::ch_error_t {
            Ok(())
        } else {
            Err(Error::from(self).into())
        }
    }
}

foreign_type! {
    /// Providing details of the compile error condition.
    pub unsafe type CompileError: Send + Sync {
        type CType = ffi::ch_compile_error_t;

        fn drop = free_compile_error;
    }
}

unsafe fn free_compile_error(err: *mut ffi::ch_compile_error_t) {
    ffi::ch_free_compile_error(err).expect("free compile error");
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl fmt::Debug for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompileError")
            .field("message", &self.message())
            .field("expression", &self.expression())
            .finish()
    }
}

impl PartialEq for CompileError {
    fn eq(&self, other: &Self) -> bool {
        self.as_ptr() == other.as_ptr()
    }
}

impl CompileError {
    unsafe fn as_ref(&self) -> &ffi::ch_compile_error_t {
        self.as_ptr().as_ref().unwrap()
    }

    /// A human-readable error message describing the error.
    pub fn message(&self) -> &str {
        unsafe { CStr::from_ptr(self.as_ref().message).to_str().unwrap() }
    }

    /// The zero-based number of the expression that caused the error (if this can be determined).
    pub fn expression(&self) -> Option<usize> {
        let n = unsafe { self.as_ref().expression };

        if n < 0 {
            None
        } else {
            Some(n as usize)
        }
    }
}

pub trait AsCompileResult {
    type Output;
    type Error: fmt::Display;

    fn ok_or(self, err: *mut ffi::ch_compile_error_t) -> Result<Self::Output, Self::Error>;
}

impl AsCompileResult for ffi::ch_error_t {
    type Output = ();
    type Error = anyhow::Error;

    fn ok_or(self, err: *mut ffi::ch_compile_error_t) -> Result<Self::Output, Self::Error> {
        if self == ffi::CH_SUCCESS as ffi::ch_error_t {
            Ok(())
        } else if self == ffi::CH_COMPILER_ERROR && !err.is_null() {
            Err(Error::CompileError(unsafe { CompileError::from_ptr(err) }).into())
        } else {
            Err(Error::from(self).into())
        }
    }
}
