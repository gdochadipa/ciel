//! Kernel-observed process authority is a prerequisite, never service admission.
use super::service_config::Principal;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DescriptorKind {
    ProtectedInput,
    Introspection,
    Null,
    Pipe,
    RuntimeSocket,
    RuntimePoll,
    RuntimeJournal,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DescriptorAccess {
    Read,
    Write,
    ReadWrite,
    Metadata,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Descriptor {
    pub number: i32,
    pub kind: DescriptorKind,
    pub access: DescriptorAccess,
    pub close_on_exec: bool,
}

pub(crate) fn validate_descriptors(descriptors: &[Descriptor]) -> Result<(), &'static str> {
    for descriptor in descriptors {
        let permitted = match descriptor.kind {
            DescriptorKind::ProtectedInput => {
                descriptor.close_on_exec
                    && matches!(
                        descriptor.access,
                        DescriptorAccess::Read | DescriptorAccess::Metadata
                    )
            }
            DescriptorKind::Introspection => {
                descriptor.close_on_exec && descriptor.access == DescriptorAccess::Read
            }
            // v1 permits only null/one-way pipes for inherited standard I/O.
            // A connected Unix socket can carry a privileged peer identity;
            // regular files can retain access granted before credential drop.
            DescriptorKind::Null => match descriptor.number {
                0 => matches!(
                    descriptor.access,
                    DescriptorAccess::Read | DescriptorAccess::ReadWrite
                ),
                1 | 2 => matches!(
                    descriptor.access,
                    DescriptorAccess::Write | DescriptorAccess::ReadWrite
                ),
                _ => false,
            },
            DescriptorKind::Pipe => match descriptor.number {
                0 => descriptor.access == DescriptorAccess::Read,
                1 | 2 => descriptor.access == DescriptorAccess::Write,
                _ => false,
            },
            DescriptorKind::Other
            | DescriptorKind::RuntimeSocket
            | DescriptorKind::RuntimePoll
            | DescriptorKind::RuntimeJournal => false,
        };
        if !permitted {
            return Err("launcher_inherited_descriptor");
        }
    }
    Ok(())
}

pub(crate) fn validate_runtime_descriptors(descriptors: &[Descriptor]) -> Result<(), &'static str> {
    for descriptor in descriptors {
        let permitted = match descriptor.kind {
            DescriptorKind::RuntimeSocket => {
                descriptor.close_on_exec && descriptor.access == DescriptorAccess::ReadWrite
            }
            DescriptorKind::RuntimePoll => {
                descriptor.close_on_exec
                    && matches!(
                        descriptor.access,
                        DescriptorAccess::Read | DescriptorAccess::ReadWrite
                    )
            }
            DescriptorKind::RuntimeJournal => {
                descriptor.close_on_exec
                    && matches!(
                        descriptor.access,
                        DescriptorAccess::Read
                            | DescriptorAccess::ReadWrite
                            | DescriptorAccess::Metadata
                    )
            }
            _ => validate_descriptors(&[*descriptor]).is_ok(),
        };
        if !permitted {
            return Err("runtime_descriptor_denied");
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LaunchAuthority {
    pub uids: [u32; 4],
    pub gids: [u32; 4],
    pub groups: Vec<u32>,
    pub capabilities: [u64; 5],
    pub no_new_privileges: bool,
    pub threads: u32,
}

impl LaunchAuthority {
    pub(crate) fn validate(&self, core: Principal) -> Result<(), &'static str> {
        self.validate_identity(core)?;
        // Run before creating an async runtime: credentials are per-thread.
        if self.threads != 1 {
            return Err("launcher_thread_state");
        }
        Ok(())
    }

    pub(crate) fn validate_identity(&self, core: Principal) -> Result<(), &'static str> {
        // Saved/filesystem IDs can retain authority even when the effective ID
        // looks correct. No root identity is accepted by this boundary.
        if core.uid == 0
            || core.gid == 0
            || self.uids != [core.uid; 4]
            || self.gids != [core.gid; 4]
        {
            return Err("launcher_process_identity_mismatch");
        }
        // initgroups may repeat the primary group. Any other group can grant
        // access absent from the administrator-selected Core identity.
        if self.groups.iter().any(|gid| *gid != core.gid) {
            return Err("launcher_supplementary_groups");
        }
        if self.capabilities != [0; 5] {
            return Err("launcher_capabilities");
        }
        if !self.no_new_privileges {
            return Err("launcher_no_new_privileges");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_handles_require_bootstrap_separation_direction_and_cloexec() {
        for kind in [
            DescriptorKind::RuntimeSocket,
            DescriptorKind::RuntimePoll,
            DescriptorKind::RuntimeJournal,
        ] {
            let descriptor = Descriptor {
                number: 8,
                kind,
                access: DescriptorAccess::ReadWrite,
                close_on_exec: true,
            };
            assert!(validate_descriptors(&[descriptor]).is_err());
            assert_eq!(validate_runtime_descriptors(&[descriptor]), Ok(()));
            assert!(validate_runtime_descriptors(&[Descriptor {
                close_on_exec: false,
                ..descriptor
            }])
            .is_err());
            assert!(validate_runtime_descriptors(&[Descriptor {
                access: DescriptorAccess::Write,
                ..descriptor
            }])
            .is_err());
        }
        let unknown = Descriptor {
            number: 8,
            kind: DescriptorKind::Other,
            access: DescriptorAccess::Read,
            close_on_exec: true,
        };
        assert!(validate_runtime_descriptors(&[unknown]).is_err());
    }

    const CORE: Principal = Principal {
        uid: 10001,
        gid: 10001,
    };

    fn authority() -> LaunchAuthority {
        LaunchAuthority {
            uids: [CORE.uid; 4],
            gids: [CORE.gid; 4],
            groups: Vec::new(),
            capabilities: [0; 5],
            no_new_privileges: true,
            threads: 1,
        }
    }

    #[test]
    fn only_the_selected_unprivileged_single_thread_identity_passes() {
        let mut observed = authority();
        assert_eq!(observed.validate(CORE), Ok(()));
        observed.groups = vec![CORE.gid];
        assert_eq!(observed.validate(CORE), Ok(()));
        observed.groups.push(10002);
        assert_eq!(
            observed.validate(CORE),
            Err("launcher_supplementary_groups")
        );
    }

    #[test]
    fn real_effective_saved_and_filesystem_ids_all_bind_to_policy() {
        for index in 0..4 {
            for uid in [0, 10002] {
                let mut observed = authority();
                observed.uids[index] = uid;
                assert_eq!(
                    observed.validate(CORE),
                    Err("launcher_process_identity_mismatch")
                );
            }
            let mut observed = authority();
            observed.gids[index] = 0;
            assert_eq!(
                observed.validate(CORE),
                Err("launcher_process_identity_mismatch")
            );
        }
        assert!(authority()
            .validate(Principal {
                uid: 0,
                gid: CORE.gid
            })
            .is_err());
        assert!(authority()
            .validate(Principal {
                uid: CORE.uid,
                gid: 0
            })
            .is_err());
    }

    #[test]
    fn every_capability_set_and_privilege_gain_path_is_rejected() {
        for index in 0..5 {
            for bit in [1, 1 << 63] {
                let mut observed = authority();
                observed.capabilities[index] = bit;
                assert_eq!(observed.validate(CORE), Err("launcher_capabilities"));
            }
        }
        let mut observed = authority();
        observed.no_new_privileges = false;
        assert_eq!(observed.validate(CORE), Err("launcher_no_new_privileges"));
        for threads in [0, 2, u32::MAX] {
            observed = authority();
            observed.threads = threads;
            assert_eq!(observed.validate(CORE), Err("launcher_thread_state"));
        }
    }

    #[test]
    fn inherited_rights_are_not_inferred_from_current_uid_or_capabilities() {
        let fd = |number, kind, access, close_on_exec| Descriptor {
            number,
            kind,
            access,
            close_on_exec,
        };
        let permitted = [
            fd(0, DescriptorKind::Null, DescriptorAccess::ReadWrite, false),
            fd(0, DescriptorKind::Pipe, DescriptorAccess::Read, false),
            fd(1, DescriptorKind::Pipe, DescriptorAccess::Write, false),
            fd(2, DescriptorKind::Pipe, DescriptorAccess::Write, false),
            fd(
                3,
                DescriptorKind::ProtectedInput,
                DescriptorAccess::Metadata,
                true,
            ),
            fd(
                4,
                DescriptorKind::ProtectedInput,
                DescriptorAccess::Read,
                true,
            ),
            fd(
                5,
                DescriptorKind::Introspection,
                DescriptorAccess::Read,
                true,
            ),
        ];
        assert_eq!(validate_descriptors(&permitted), Ok(()));
        for denied in [
            fd(3, DescriptorKind::Null, DescriptorAccess::ReadWrite, false),
            fd(0, DescriptorKind::Null, DescriptorAccess::Write, false),
            fd(1, DescriptorKind::Null, DescriptorAccess::Read, false),
            fd(2, DescriptorKind::Null, DescriptorAccess::Metadata, true),
            fd(3, DescriptorKind::Other, DescriptorAccess::Read, true),
            fd(1, DescriptorKind::Other, DescriptorAccess::Write, false),
            fd(3, DescriptorKind::Pipe, DescriptorAccess::Read, true),
            fd(0, DescriptorKind::Pipe, DescriptorAccess::Write, false),
            fd(1, DescriptorKind::Pipe, DescriptorAccess::ReadWrite, false),
            fd(
                4,
                DescriptorKind::ProtectedInput,
                DescriptorAccess::Write,
                true,
            ),
            fd(
                4,
                DescriptorKind::ProtectedInput,
                DescriptorAccess::ReadWrite,
                true,
            ),
            fd(
                4,
                DescriptorKind::ProtectedInput,
                DescriptorAccess::Read,
                false,
            ),
            fd(
                5,
                DescriptorKind::Introspection,
                DescriptorAccess::Read,
                false,
            ),
        ] {
            assert_eq!(
                validate_descriptors(&[denied]),
                Err("launcher_inherited_descriptor")
            );
        }
    }
}
