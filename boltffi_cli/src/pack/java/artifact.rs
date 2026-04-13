use std::path::Path;

use crate::cargo::CargoMetadata;
use crate::error::Result;

use super::plan::JvmCrateOutputs;

impl JvmCrateOutputs {
    pub(crate) fn from_metadata(
        metadata: &CargoMetadata,
        crate_artifact_name: &str,
        manifest_path: &Path,
        package_selector: Option<&str>,
    ) -> Result<Self> {
        let package = metadata.find_package(manifest_path, package_selector)?;
        let target = package.resolve_library_target(crate_artifact_name, manifest_path)?;

        Ok(Self {
            builds_staticlib: target.builds_staticlib(),
            builds_cdylib: target.builds_cdylib(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::JvmCrateOutputs;
    use crate::cargo::CargoMetadata;

    fn metadata(json: &str) -> CargoMetadata {
        serde_json::from_str(json).expect("cargo metadata fixture")
    }

    #[test]
    fn parses_current_jvm_crate_outputs_from_cargo_metadata() {
        let metadata = metadata(
            r#"{
                "target_directory": "/tmp/boltffi-target",
                "packages": [
                    {
                        "id": "path+file:///tmp/workspace/sibling#0.1.0",
                        "name": "sibling",
                        "manifest_path": "/tmp/workspace/sibling/Cargo.toml",
                        "targets": [{
                            "name": "demo",
                            "crate_types": ["cdylib"]
                        }]
                    },
                    {
                        "id": "path+file:///tmp/workspace/current#0.1.0",
                        "name": "current",
                        "manifest_path": "/tmp/workspace/current/Cargo.toml",
                        "targets": [
                            {
                                "name": "demo",
                                "crate_types": ["staticlib", "cdylib", "rlib"]
                            },
                            {
                                "name": "demo_cli",
                                "crate_types": ["bin"]
                            }
                        ]
                    }
                ]
            }"#,
        );

        let outputs = JvmCrateOutputs::from_metadata(
            &metadata,
            "demo",
            Path::new("/tmp/workspace/current/Cargo.toml"),
            None,
        )
        .expect("crate outputs");

        assert_eq!(
            outputs,
            JvmCrateOutputs {
                builds_staticlib: true,
                builds_cdylib: true,
            }
        );
    }

    #[test]
    fn scopes_jvm_crate_outputs_to_selected_package_manifest() {
        let metadata = metadata(
            r#"{
                "target_directory": "/tmp/boltffi-target",
                "packages": [
                    {
                        "id": "path+file:///tmp/workspace/a#0.1.0",
                        "name": "workspace-a",
                        "manifest_path": "/tmp/workspace/a/Cargo.toml",
                        "targets": [{
                            "name": "shared_name",
                            "crate_types": ["cdylib"]
                        }]
                    },
                    {
                        "id": "path+file:///tmp/workspace/b#0.1.0",
                        "name": "workspace-b",
                        "manifest_path": "/tmp/workspace/b/Cargo.toml",
                        "targets": [{
                            "name": "shared_name",
                            "crate_types": ["staticlib"]
                        }]
                    }
                ]
            }"#,
        );

        let outputs = JvmCrateOutputs::from_metadata(
            &metadata,
            "shared_name",
            Path::new("/tmp/workspace/b/Cargo.toml"),
            None,
        )
        .expect("crate outputs");

        assert_eq!(
            outputs,
            JvmCrateOutputs {
                builds_staticlib: true,
                builds_cdylib: false,
            }
        );
    }

    #[test]
    fn scopes_jvm_crate_outputs_to_selected_package_name() {
        let metadata = metadata(
            r#"{
                "target_directory": "/tmp/boltffi-target",
                "packages": [
                    {
                        "id": "path+file:///tmp/workspace#workspace-a@0.1.0",
                        "name": "workspace-a",
                        "manifest_path": "/tmp/workspace/Cargo.toml",
                        "targets": [{
                            "name": "shared_name",
                            "crate_types": ["cdylib"]
                        }]
                    },
                    {
                        "id": "path+file:///tmp/workspace#workspace-b@0.1.0",
                        "name": "workspace-b",
                        "manifest_path": "/tmp/workspace/Cargo.toml",
                        "targets": [{
                            "name": "shared_name",
                            "crate_types": ["staticlib"]
                        }]
                    }
                ]
            }"#,
        );

        let outputs = JvmCrateOutputs::from_metadata(
            &metadata,
            "shared_name",
            Path::new("/tmp/workspace/Cargo.toml"),
            Some("workspace-b"),
        )
        .expect("crate outputs");

        assert_eq!(
            outputs,
            JvmCrateOutputs {
                builds_staticlib: true,
                builds_cdylib: false,
            }
        );
    }

    #[test]
    fn falls_back_to_selected_package_ffi_target_when_preferred_artifact_name_differs() {
        let metadata = metadata(
            r#"{
                "target_directory": "/tmp/boltffi-target",
                "packages": [{
                    "id": "path+file:///tmp/workspace/member#0.1.0",
                    "name": "workspace-member",
                    "manifest_path": "/tmp/workspace/member/Cargo.toml",
                    "targets": [
                        {
                            "name": "workspace_member_lib",
                            "crate_types": ["staticlib", "cdylib"]
                        },
                        {
                            "name": "workspace_member_cli",
                            "crate_types": ["bin"]
                        }
                    ]
                }]
            }"#,
        );

        let outputs = JvmCrateOutputs::from_metadata(
            &metadata,
            "root_config_name",
            Path::new("/tmp/workspace/Cargo.toml"),
            Some("workspace-member"),
        )
        .expect("crate outputs");

        assert_eq!(
            outputs,
            JvmCrateOutputs {
                builds_staticlib: true,
                builds_cdylib: true,
            }
        );
    }
}
