//! Reusable-task contracts and RTIC-shaped authoring placeholders.
//!
//! A task contract names the resources, compile-time configuration, and spawn
//! targets used by an ordinary Rust function. The generated context types in
//! this module exist only to give Rust and rust-analyzer a concrete authoring
//! surface. A later generator will copy the checked function body into a real
//! RTIC task and map the logical names through an application composition.

use core::marker::PhantomData;

use fugit::MillisDurationU32;

/// Describes one named local or shared resource required by a task body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceRequirement {
    /// Logical field name used below `cx.local` or `cx.shared`.
    pub id: &'static str,

    /// Rust type or trait-object spelling used by the authoring context.
    pub rust_type: &'static str,
}

impl ResourceRequirement {
    /// Creates a resource requirement.
    pub const fn new(id: &'static str, rust_type: &'static str) -> Self {
        Self { id, rust_type }
    }
}

/// Describes one composition-provided constant used below `cx.config`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigRequirement {
    /// Logical configuration field name.
    pub id: &'static str,

    /// Rust type spelling expected by the task body.
    pub rust_type: &'static str,
}

impl ConfigRequirement {
    /// Creates a configuration requirement.
    pub const fn new(id: &'static str, rust_type: &'static str) -> Self {
        Self { id, rust_type }
    }
}

/// Describes one argument passed through an RTIC-style spawn call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpawnArgument {
    /// Argument name used by the reusable task contract.
    pub id: &'static str,

    /// Rust type spelling required by the spawn target.
    pub rust_type: &'static str,
}

impl SpawnArgument {
    /// Creates a spawn argument description.
    pub const fn new(id: &'static str, rust_type: &'static str) -> Self {
        Self { id, rust_type }
    }
}

/// Describes one logical task that the reusable body may spawn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpawnRequirement {
    /// Logical module name used by calls such as `worker::spawn(...)`.
    pub id: &'static str,

    /// Ordered arguments accepted by the spawn call.
    pub arguments: &'static [SpawnArgument],
}

impl SpawnRequirement {
    /// Creates a spawn requirement.
    pub const fn new(id: &'static str, arguments: &'static [SpawnArgument]) -> Self {
        Self { id, arguments }
    }
}

/// Location of the ordinary Rust function implementing a task contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskSource {
    /// Source file containing the `reusable_task!` invocation.
    pub file: &'static str,

    /// Rust function extracted from the invocation during generation.
    pub function: &'static str,
}

impl TaskSource {
    /// Creates task-source metadata recorded at the macro invocation site.
    pub const fn new(file: &'static str, function: &'static str) -> Self {
        Self { file, function }
    }
}

/// Complete logical interface owned by one reusable task definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskContract {
    /// Reusable function and generated RTIC context name.
    pub id: &'static str,

    /// Source location of the checked ordinary Rust task body.
    pub source: TaskSource,

    /// Resources owned exclusively by the concrete task instance.
    pub local: &'static [ResourceRequirement],

    /// Resources accessed through RTIC-style shared locking.
    pub shared: &'static [ResourceRequirement],

    /// Compile-time values supplied by application composition.
    pub config: &'static [ConfigRequirement],

    /// Logical task-to-task spawn connections supplied by composition.
    pub spawns: &'static [SpawnRequirement],
}

/// Mutable authoring proxy with the same closure-based shape as RTIC locking.
pub struct SharedResource<'a, T: ?Sized> {
    value: &'a mut T,
}

impl<'a, T: ?Sized> SharedResource<'a, T> {
    /// Wraps a mutable value for host-side task checking.
    pub const fn new(value: &'a mut T) -> Self {
        Self { value }
    }

    /// Executes one bounded critical-section-shaped access to the value.
    pub fn lock<R>(&mut self, operation: impl FnOnce(&mut T) -> R) -> R {
        operation(self.value)
    }
}

/// Failure returned by an authoring-only spawn placeholder.
///
/// The placeholder currently never fails. The result exists so reusable code
/// can handle a call with the same shape as an RTIC spawn operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpawnError;

/// Authoring-only stand-in for the RTIC application's monotonic timer.
pub struct Mono;

impl Mono {
    /// Waits for a millisecond duration using RTIC-shaped async syntax.
    ///
    /// This placeholder is only type-checked; reusable task tests must not poll
    /// an infinite task loop that awaits it. The generated application will
    /// resolve `Mono` to its actual RTIC monotonic.
    pub async fn delay(_duration: MillisDurationU32) {
        core::future::pending::<()>().await;
    }
}

/// One concrete binding for a task-local logical resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalResourceBinding {
    logical: &'static str,
    target: &'static str,
    rust_type: &'static str,
}

impl LocalResourceBinding {
    /// Returns the logical field named by the task contract.
    pub const fn logical(self) -> &'static str {
        self.logical
    }

    /// Returns the concrete hardware-resource identifier selected by composition.
    pub const fn target(self) -> &'static str {
        self.target
    }

    /// Returns the capability type required by the task contract.
    pub const fn rust_type(self) -> &'static str {
        self.rust_type
    }
}

/// One concrete binding for a task-shared logical resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SharedResourceBinding {
    logical: &'static str,
    target: &'static str,
    rust_type: &'static str,
}

impl SharedResourceBinding {
    /// Returns the logical field named by the task contract.
    pub const fn logical(self) -> &'static str {
        self.logical
    }

    /// Returns the application shared-resource identifier selected by composition.
    pub const fn target(self) -> &'static str {
        self.target
    }

    /// Returns the Rust type required by the task contract.
    pub const fn rust_type(self) -> &'static str {
        self.rust_type
    }
}

/// Compile-time values supported by the initial composition prototype.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigValue {
    /// Boolean constant.
    Bool(bool),

    /// Unsigned 32-bit integer constant.
    U32(u32),

    /// Millisecond duration represented by `fugit`.
    Millis(MillisDurationU32),
}

impl ConfigValue {
    /// Returns the Rust type spelling represented by this value.
    pub const fn rust_type(self) -> &'static str {
        match self {
            Self::Bool(_) => "bool",
            Self::U32(_) => "u32",
            Self::Millis(_) => "MillisDurationU32",
        }
    }
}

/// One composition-provided value for a logical `cx.config` field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigBinding {
    logical: &'static str,
    expected_type: &'static str,
    value: ConfigValue,
}

impl ConfigBinding {
    /// Returns the logical field named by the task contract.
    pub const fn logical(self) -> &'static str {
        self.logical
    }

    /// Returns the Rust type required by the task contract.
    pub const fn expected_type(self) -> &'static str {
        self.expected_type
    }

    /// Returns the concrete compile-time value selected by composition.
    pub const fn value(self) -> ConfigValue {
        self.value
    }
}

/// One concrete destination for a logical RTIC-style spawn call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpawnBinding {
    logical: &'static str,
    target: &'static str,
    arguments: &'static [SpawnArgument],
}

impl SpawnBinding {
    /// Returns the logical spawn module used by the reusable body.
    pub const fn logical(self) -> &'static str {
        self.logical
    }

    /// Returns the concrete task identifier selected by composition.
    pub const fn target(self) -> &'static str {
        self.target
    }

    /// Returns the ordered argument contract of the logical spawn call.
    pub const fn arguments(self) -> &'static [SpawnArgument] {
        self.arguments
    }
}

/// Typed task-local slot generated from a task contract.
pub struct LocalSlot<T: ?Sized> {
    id: &'static str,
    rust_type: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T: ?Sized> LocalSlot<T> {
    /// Creates a typed task-local slot.
    pub const fn new(id: &'static str, rust_type: &'static str) -> Self {
        Self {
            id,
            rust_type,
            marker: PhantomData,
        }
    }

    /// Binds the slot to one concrete hardware-resource identifier.
    pub const fn bind(&self, target: &'static str) -> LocalResourceBinding {
        LocalResourceBinding {
            logical: self.id,
            target,
            rust_type: self.rust_type,
        }
    }
}

/// Typed task-shared slot generated from a task contract.
pub struct SharedSlot<T: ?Sized> {
    id: &'static str,
    rust_type: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T: ?Sized> SharedSlot<T> {
    /// Creates a typed task-shared slot.
    pub const fn new(id: &'static str, rust_type: &'static str) -> Self {
        Self {
            id,
            rust_type,
            marker: PhantomData,
        }
    }

    /// Binds the slot to one concrete application shared resource.
    pub const fn bind(&self, target: &'static str) -> SharedResourceBinding {
        SharedResourceBinding {
            logical: self.id,
            target,
            rust_type: self.rust_type,
        }
    }
}

/// Typed `cx.config` slot generated from a task contract.
pub struct ConfigSlot<T> {
    id: &'static str,
    rust_type: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T> ConfigSlot<T> {
    /// Creates a typed configuration slot.
    pub const fn new(id: &'static str, rust_type: &'static str) -> Self {
        Self {
            id,
            rust_type,
            marker: PhantomData,
        }
    }
}

impl ConfigSlot<bool> {
    /// Supplies a Boolean value to this configuration slot.
    pub const fn set(&self, value: bool) -> ConfigBinding {
        ConfigBinding {
            logical: self.id,
            expected_type: self.rust_type,
            value: ConfigValue::Bool(value),
        }
    }
}

impl ConfigSlot<u32> {
    /// Supplies an unsigned 32-bit value to this configuration slot.
    pub const fn set(&self, value: u32) -> ConfigBinding {
        ConfigBinding {
            logical: self.id,
            expected_type: self.rust_type,
            value: ConfigValue::U32(value),
        }
    }
}

impl ConfigSlot<MillisDurationU32> {
    /// Supplies a millisecond duration to this configuration slot.
    pub const fn set(&self, value: MillisDurationU32) -> ConfigBinding {
        ConfigBinding {
            logical: self.id,
            expected_type: self.rust_type,
            value: ConfigValue::Millis(value),
        }
    }
}

/// Typed logical spawn slot generated from a task contract.
pub struct SpawnSlot {
    id: &'static str,
    arguments: &'static [SpawnArgument],
}

impl SpawnSlot {
    /// Creates a logical spawn slot.
    pub const fn new(id: &'static str, arguments: &'static [SpawnArgument]) -> Self {
        Self { id, arguments }
    }

    /// Binds this logical spawn operation to a concrete task identifier.
    pub const fn bind(&self, target: &'static str) -> SpawnBinding {
        SpawnBinding {
            logical: self.id,
            target,
            arguments: self.arguments,
        }
    }
}

/// Declares one reusable task contract together with its ordinary Rust body.
///
/// The enclosed function name becomes the contract ID and generated context
/// module name. Synchronous and asynchronous task functions are supported.
#[macro_export]
macro_rules! reusable_task {
    (
        contract {
            local {
                $(
                    $(#[$local_attribute:meta])*
                    $local:ident : $local_type:ty
                ),* $(,)?
            }
            shared {
                $(
                    $(#[$shared_attribute:meta])*
                    $shared:ident : $shared_type:ty
                ),* $(,)?
            }
            config {
                $(
                    $(#[$config_attribute:meta])*
                    $config:ident : $config_type:ty
                ),* $(,)?
            }
            spawns {
                $(
                    $(#[$spawn_attribute:meta])*
                    $spawn:ident (
                        $($spawn_argument:ident : $spawn_argument_type:ty),* $(,)?
                    )
                ),* $(,)?
            }
        }

        $(#[$function_attribute:meta])*
        $visibility:vis async fn $task:ident (
            $($function_argument:tt)*
        ) $(-> $return_type:ty)?
        $body:block
    ) => {
        $crate::task_contract! {
            $visibility task $task {
                local {
                    $(
                        $(#[$local_attribute])*
                        $local: $local_type,
                    )*
                }
                shared {
                    $(
                        $(#[$shared_attribute])*
                        $shared: $shared_type,
                    )*
                }
                config {
                    $(
                        $(#[$config_attribute])*
                        $config: $config_type,
                    )*
                }
                spawns {
                    $(
                        $(#[$spawn_attribute])*
                        $spawn($($spawn_argument: $spawn_argument_type),*),
                    )*
                }
            }
        }

        $(#[$function_attribute])*
        $visibility async fn $task(
            $($function_argument)*
        ) $(-> $return_type)?
        $body
    };

    (
        contract {
            local {
                $(
                    $(#[$local_attribute:meta])*
                    $local:ident : $local_type:ty
                ),* $(,)?
            }
            shared {
                $(
                    $(#[$shared_attribute:meta])*
                    $shared:ident : $shared_type:ty
                ),* $(,)?
            }
            config {
                $(
                    $(#[$config_attribute:meta])*
                    $config:ident : $config_type:ty
                ),* $(,)?
            }
            spawns {
                $(
                    $(#[$spawn_attribute:meta])*
                    $spawn:ident (
                        $($spawn_argument:ident : $spawn_argument_type:ty),* $(,)?
                    )
                ),* $(,)?
            }
        }

        $(#[$function_attribute:meta])*
        $visibility:vis fn $task:ident (
            $($function_argument:tt)*
        ) $(-> $return_type:ty)?
        $body:block
    ) => {
        $crate::task_contract! {
            $visibility task $task {
                local {
                    $(
                        $(#[$local_attribute])*
                        $local: $local_type,
                    )*
                }
                shared {
                    $(
                        $(#[$shared_attribute])*
                        $shared: $shared_type,
                    )*
                }
                config {
                    $(
                        $(#[$config_attribute])*
                        $config: $config_type,
                    )*
                }
                spawns {
                    $(
                        $(#[$spawn_attribute])*
                        $spawn($($spawn_argument: $spawn_argument_type),*),
                    )*
                }
            }
        }

        $(#[$function_attribute])*
        $visibility fn $task(
            $($function_argument)*
        ) $(-> $return_type)?
        $body
    };
}

/// Declares a task-local contract and its RTIC-shaped authoring context.
///
/// The macro deliberately does not declare the task function. Authors write
/// that function as a normal Rust item immediately after the contract and use
/// the generated `<task>::Context` type in its signature.
#[macro_export]
macro_rules! task_contract {
    (
        $(#[$task_attribute:meta])*
        $visibility:vis task $task:ident {
            local {
                $(
                    $(#[$local_attribute:meta])*
                    $local:ident : $local_type:ty
                ),* $(,)?
            }
            shared {
                $(
                    $(#[$shared_attribute:meta])*
                    $shared:ident : $shared_type:ty
                ),* $(,)?
            }
            config {
                $(
                    $(#[$config_attribute:meta])*
                    $config:ident : $config_type:ty
                ),* $(,)?
            }
            spawns {
                $(
                    $(#[$spawn_attribute:meta])*
                    $spawn:ident (
                        $($argument:ident : $argument_type:ty),* $(,)?
                    )
                ),* $(,)?
            }
        }
    ) => {
        $(#[$task_attribute])*
        #[doc = concat!("Authoring context and contract for task `", stringify!($task), "`.")]
        $visibility mod $task {
            #[allow(unused_imports)]
            use super::*;

            /// Task-local resources exposed below `cx.local`.
            pub struct Local<'a> {
                $(
                    $(#[$local_attribute])*
                    #[doc = concat!("Task-local resource `", stringify!($local), "`.")]
                    pub $local: &'a mut $local_type,
                )*
                marker: ::core::marker::PhantomData<&'a mut ()>,
            }

            #[allow(clippy::new_without_default)]
            impl<'a> Local<'a> {
                /// Creates the authoring view of the task's local resources.
                pub fn new($($local: &'a mut $local_type),*) -> Self {
                    Self {
                        $($local,)*
                        marker: ::core::marker::PhantomData,
                    }
                }
            }

            /// Task-shared resources exposed below `cx.shared`.
            pub struct Shared<'a> {
                $(
                    $(#[$shared_attribute])*
                    #[doc = concat!("Task-shared resource `", stringify!($shared), "`.")]
                    pub $shared: $crate::rtic::task::SharedResource<'a, $shared_type>,
                )*
                marker: ::core::marker::PhantomData<&'a mut ()>,
            }

            #[allow(clippy::new_without_default)]
            impl<'a> Shared<'a> {
                /// Creates the authoring view of the task's shared resources.
                pub fn new($($shared: &'a mut $shared_type),*) -> Self {
                    Self {
                        $($shared: $crate::rtic::task::SharedResource::new($shared),)*
                        marker: ::core::marker::PhantomData,
                    }
                }
            }

            /// Composition-provided constants exposed below `cx.config`.
            pub struct Config {
                $(
                    $(#[$config_attribute])*
                    #[doc = concat!("Task configuration value `", stringify!($config), "`.")]
                    pub $config: $config_type,
                )*
            }

            #[allow(clippy::new_without_default)]
            impl Config {
                /// Creates the authoring view of the task's constant configuration.
                pub const fn new($($config: $config_type),*) -> Self {
                    Self { $($config,)* }
                }
            }

            /// Dummy context used to type-check the reusable task as normal Rust.
            pub struct Context<'a> {
                /// Resources owned exclusively by this task instance.
                pub local: Local<'a>,

                /// Resources accessed through RTIC-shaped locking.
                pub shared: Shared<'a>,

                /// Compile-time values selected by application composition.
                pub config: Config,
            }

            impl<'a> Context<'a> {
                /// Creates a complete authoring context.
                pub const fn new(local: Local<'a>, shared: Shared<'a>, config: Config) -> Self {
                    Self {
                        local,
                        shared,
                        config,
                    }
                }
            }

            /// Typed handles for task-local composition bindings.
            pub struct LocalSlots {
                $(
                    #[doc = concat!("Binding handle for local resource `", stringify!($local), "`.")]
                    pub $local: $crate::rtic::task::LocalSlot<$local_type>,
                )*
            }

            /// Typed task-local slots used by application composition.
            pub const LOCAL: LocalSlots = LocalSlots {
                $(
                    $local: $crate::rtic::task::LocalSlot::new(
                        stringify!($local),
                        stringify!($local_type),
                    ),
                )*
            };

            /// Typed handles for task-shared composition bindings.
            pub struct SharedSlots {
                $(
                    #[doc = concat!("Binding handle for shared resource `", stringify!($shared), "`.")]
                    pub $shared: $crate::rtic::task::SharedSlot<$shared_type>,
                )*
            }

            /// Typed task-shared slots used by application composition.
            pub const SHARED: SharedSlots = SharedSlots {
                $(
                    $shared: $crate::rtic::task::SharedSlot::new(
                        stringify!($shared),
                        stringify!($shared_type),
                    ),
                )*
            };

            /// Typed handles for composition-provided task constants.
            pub struct ConfigSlots {
                $(
                    #[doc = concat!("Binding handle for configuration value `", stringify!($config), "`.")]
                    pub $config: $crate::rtic::task::ConfigSlot<$config_type>,
                )*
            }

            /// Typed configuration slots used by application composition.
            pub const CONFIG: ConfigSlots = ConfigSlots {
                $(
                    $config: $crate::rtic::task::ConfigSlot::new(
                        stringify!($config),
                        stringify!($config_type),
                    ),
                )*
            };

            /// Typed handles for logical task spawn bindings.
            pub struct SpawnSlots {
                $(
                    #[doc = concat!("Binding handle for logical spawn `", stringify!($spawn), "`.")]
                    pub $spawn: $crate::rtic::task::SpawnSlot,
                )*
            }

            /// Typed spawn slots used by application composition.
            pub const SPAWNS: SpawnSlots = SpawnSlots {
                $(
                    $spawn: $crate::rtic::task::SpawnSlot::new(
                        stringify!($spawn),
                        &[
                            $(
                                $crate::rtic::task::SpawnArgument::new(
                                    stringify!($argument),
                                    stringify!($argument_type),
                                ),
                            )*
                        ],
                    ),
                )*
            };

            /// Machine-readable contract consumed by composition validation.
            pub const CONTRACT: $crate::rtic::task::TaskContract =
                $crate::rtic::task::TaskContract {
                    id: stringify!($task),
                    source: $crate::rtic::task::TaskSource::new(
                        file!(),
                        stringify!($task),
                    ),
                    local: &[
                        $(
                            $crate::rtic::task::ResourceRequirement::new(
                                stringify!($local),
                                stringify!($local_type),
                            ),
                        )*
                    ],
                    shared: &[
                        $(
                            $crate::rtic::task::ResourceRequirement::new(
                                stringify!($shared),
                                stringify!($shared_type),
                            ),
                        )*
                    ],
                    config: &[
                        $(
                            $crate::rtic::task::ConfigRequirement::new(
                                stringify!($config),
                                stringify!($config_type),
                            ),
                        )*
                    ],
                    spawns: &[
                        $(
                            $crate::rtic::task::SpawnRequirement::new(
                                stringify!($spawn),
                                &[
                                    $(
                                        $crate::rtic::task::SpawnArgument::new(
                                            stringify!($argument),
                                            stringify!($argument_type),
                                        ),
                                    )*
                                ],
                            ),
                        )*
                    ],
                };
        }

        $(
            $(#[$spawn_attribute])*
            mod $spawn {
                /// Host-only placeholder for an RTIC-generated spawn function.
                pub fn spawn(
                    $($argument: $argument_type),*
                ) -> Result<(), $crate::rtic::task::SpawnError> {
                    let _ = ($($argument,)*);
                    Ok(())
                }
            }
        )*
    };
}

#[cfg(test)]
mod tests {
    use crate::tasks;

    #[test]
    fn reusable_task_records_its_source_location() {
        let contract = tasks::blink_led::CONTRACT;

        assert_eq!(contract.source.function, contract.id);
        assert!(contract.source.file.ends_with("src/tasks/blink_led.rs"));
    }
}
