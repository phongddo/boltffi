use crate::{
    ir::{AbiContract, FfiContract},
    render::dart::DartLibrary,
};

pub struct DartLowerer<'a> {
    ffi: &'a FfiContract,
    abi: &'a AbiContract,
    package_name: &'a str,
}

impl<'a> DartLowerer<'a> {
    pub fn new(ffi: &'a FfiContract, abi: &'a AbiContract, package_name: &'a str) -> Self {
        Self {
            ffi,
            abi,
            package_name,
        }
    }

    pub fn library(&self) -> DartLibrary {
        DartLibrary {}
    }
}
