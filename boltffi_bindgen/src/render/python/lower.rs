use crate::ir::{AbiContract, FfiContract};
use crate::render::python::{PythonExportCounts, PythonModule};

pub struct PythonLowerer<'a> {
    ffi_contract: &'a FfiContract,
    abi_contract: &'a AbiContract,
    module_name: &'a str,
}

impl<'a> PythonLowerer<'a> {
    pub fn new(
        ffi_contract: &'a FfiContract,
        abi_contract: &'a AbiContract,
        module_name: &'a str,
    ) -> Self {
        Self {
            ffi_contract,
            abi_contract,
            module_name,
        }
    }

    pub fn lower(&self) -> PythonModule {
        let exported_api = PythonExportCounts {
            functions: self.ffi_contract.functions.len(),
            records: self.ffi_contract.catalog.all_records().count(),
            enumerations: self.ffi_contract.catalog.all_enums().count(),
            classes: self.ffi_contract.catalog.all_classes().count(),
            callbacks: self.abi_contract.callbacks.len(),
        };

        PythonModule {
            module_name: self.module_name.to_string(),
            package_name: self.ffi_contract.package.name.clone(),
            package_version: self.ffi_contract.package.version.clone(),
            exported_api,
        }
    }
}
