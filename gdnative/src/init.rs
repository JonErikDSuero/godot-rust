//! Types and functionalities to declare and initialize gdnative classes.
//!
//! ## API endpoints
//!
//! Three endpoints are automatically invoked by the engine during startup and shutdown:
//!
//! - [`godot_gdnative_init`](macro.godot_gdnative_init.html),
//! - [`godot_nativescript_init`](macro.godot_nativescript_init.html),
//! - [`godot_gdnative_terminate`](macro.godot_gdnative_terminate.html),
//!
//! All three must be present.
//!
//! ## Registering a class using the `godot_class` macro
//!
//! See the [spinning_cube example](https://github.com/GodotNativeTools/godot-rust/tree/master/examples/spinning_cube)
//! in the repositiory.
//!
//! ## Registering a class manually
//!
//! See the [manually_registered example](https://github.com/GodotNativeTools/godot-rust/tree/master/examples/manually_registered)
//! in the repositiory.
//!

use super::*;
use get_api;
use Variant;
use GodotType;
use NativeClass;
use sys::godot_property_usage_flags::*;
use sys::godot_property_hint::*;
use std::mem;
use std::ops::Range;
use std::ffi::CString;
use std::marker::PhantomData;
use std::ptr;
use libc;

/// A handle that can register new classes to the engine during initialization.
///
/// See [`godot_nativescript_init`](macro.godot_nativescript_init.html).
#[derive(Copy, Clone)]
pub struct InitHandle {
    #[doc(hidden)]
    handle: *mut libc::c_void,
}

impl InitHandle {
    #[doc(hidden)]
    pub unsafe fn new(handle: *mut libc::c_void) -> Self { InitHandle { handle } }

    /// Registers a new class to the engine.
    ///
    /// The return `ClassBuilder` can be used to add methods, signals and properties
    /// to the class.
    pub fn add_class<C>(&self, desc: ClassDescriptor) -> ClassBuilder<C>
    where C: NativeClass {
        unsafe {
            let class_name = CString::new(desc.name).unwrap();
            let base_name = CString::new(desc.base_class).unwrap();

            let create = sys::godot_instance_create_func {
                create_func: desc.constructor,
                method_data: ptr::null_mut(),
                free_func: None,
            };

            let destroy = sys::godot_instance_destroy_func {
                destroy_func: desc.destructor,
                method_data: ptr::null_mut(),
                free_func: None,
            };

            (get_api().godot_nativescript_register_class)(
                self.handle as *mut _,
                class_name.as_ptr() as *const _,
                base_name.as_ptr() as *const _,
                create,
                destroy
            );

            ClassBuilder {
                init_handle: self.handle,
                class_name,
                _marker: PhantomData,
            }
        }
    }
}

pub type ScriptMethodFn = unsafe extern "C" fn(
    *mut sys::godot_object,
    *mut libc::c_void,
    *mut libc::c_void,
    libc::c_int,
    *mut *mut sys::godot_variant
) -> sys::godot_variant;

pub type ScriptConstructorFn = unsafe extern "C" fn(
    *mut sys::godot_object,
    *mut libc::c_void
) -> *mut libc::c_void;

pub type ScriptDestructorFn = unsafe extern "C" fn(
    *mut sys::godot_object,
    *mut libc::c_void,
    *mut libc::c_void
) -> ();

pub enum RpcMode {
    Disabled,
    Remote,
    Sync,
    Mater,
    Slave
}

pub struct ScriptMethodAttributes {
    pub rpc_mode: RpcMode
}

pub struct ScriptMethod<'l> {
    pub name: &'l str,
    pub method_ptr: Option<ScriptMethodFn>,
    pub attributes: ScriptMethodAttributes,

    pub method_data: *mut libc::c_void,
    pub free_func: Option<unsafe extern "C" fn(*mut libc::c_void) -> ()>,
}

pub struct ClassDescriptor<'l> {
    pub name: &'l str,
    pub base_class: &'l str,
    pub constructor: Option<ScriptConstructorFn>,
    pub destructor: Option<ScriptDestructorFn>,
}

pub struct ClassBuilder<C: NativeClass> {
    #[doc(hidden)]
    pub init_handle: *mut libc::c_void,
    class_name: CString,
    _marker: PhantomData<C>,
}

impl<C: NativeClass> ClassBuilder<C> {

    pub fn add_method_advanced(&self, method: ScriptMethod) {
        let method_name = CString::new(method.name).unwrap();
        let attr = sys::godot_method_attributes {
            rpc_type: sys::godot_method_rpc_mode::GODOT_METHOD_RPC_MODE_DISABLED
        };

        let method_desc = sys::godot_instance_method {
            method: method.method_ptr,
            method_data: method.method_data,
            free_func: method.free_func
        };

        unsafe {
            (get_api().godot_nativescript_register_method)(
                self.init_handle,
                self.class_name.as_ptr() as *const _,
                method_name.as_ptr() as *const _,
                attr,
                method_desc
            );
        }
    }

    pub fn add_method(&self, name: &str, method: ScriptMethodFn) {
        self.add_method_advanced(
            ScriptMethod {
                name: name,
                method_ptr: Some(method),
                attributes: ScriptMethodAttributes {
                    rpc_mode: RpcMode::Disabled
                },
                method_data: ptr::null_mut(),
                free_func: None
            },
        );
    }

    pub fn add_property<T, S, G>(&self, property: Property<T, S, G>)
    where
        T: GodotType,
        S: PropertySetter<C, T>,
        G: PropertyGetter<C, T>,
    {
        unsafe {
            let hint_text = match property.hint {
                PropertyHint::Range { ref range, step, slider } => {

                    if slider {
                        Some(format!("{},{},{},slider", range.start, range.end, step))
                    } else {
                        Some(format!("{},{},{}", range.start, range.end, step))
                    }
                }
                PropertyHint::Enum { values } | PropertyHint::Flags { values } => { Some(values.join(",")) }
                PropertyHint::NodePathToEditedNode | PropertyHint::None => { None }
            };
            let hint_string = if let Some(text) = hint_text {
                GodotString::from_str(text)
            } else {
                GodotString::default()
            };

            let default: Variant = property.default.to_variant();
            let ty = default.get_type();

            let mut attr = sys::godot_property_attributes {
                rset_type: sys::godot_method_rpc_mode::GODOT_METHOD_RPC_MODE_DISABLED, // TODO:
                type_: mem::transmute(ty),
                hint: property.hint.to_sys(),
                hint_string: hint_string.to_sys(),
                usage: property.usage.to_sys(),
                default_value: default.to_sys(),
            };

            let path = ::std::ffi::CString::new(property.name).unwrap();

            let set = property.setter.as_godot_function();
            let get = property.getter.as_godot_function();

            (get_api().godot_nativescript_register_property)(
                self.init_handle,
                self.class_name.as_ptr(),
                path.as_ptr() as *const _,
                &mut attr, set, get
            );
        }
    }

    pub fn add_signal(&self, signal: Signal) {
        use std::ptr;
        unsafe {
            let name = GodotString::from_str(signal.name);
            (get_api().godot_nativescript_register_signal)(
                self.init_handle,
                self.class_name.as_ptr(),
                &sys::godot_signal {
                    name: name.to_sys(),
                    num_args: 0,
                    args: ptr::null_mut(),
                    num_default_args: 0,
                    default_args: ptr::null_mut(),
                }
            );
        }
    }
}

// TODO: missing property hints.
pub enum PropertyHint<'l> {
    None,
    Range {
        range: Range<f64>,
        step: f64,
        slider: bool,
    },
    // ExpRange,
    Enum {
        values: &'l[&'l str],
    },
    // ExpEasing,
    // Length,
    // SpriteFrame,
    // KeyAccel,
    Flags {
        values: &'l[&'l str],
    },
    // Layers2DRender,
    // Layers2DPhysics,
    // Layers3DRender,
    // Layers3DPhysics,
    // File,
    // Dir,
    // GlobalFile,
    // GlobalDir,
    // ResourceType,
    // MultilineText,
    // ColorNoAlpha,
    // ImageCompressLossy,
    // IMageCompressLossless,
    // ObjectID,
    // TypeString,
    NodePathToEditedNode,
    // MethodOfVariantType,
    // MethodOfBaseType,
    // MethodOfInstance,
    // MethodOfScript,
    // PropertyOfVariantType,
    // PropertyOfBaseType,
    // PropertyOfInstance,
    // PropertyOfScript,
}

impl<'l> PropertyHint<'l> {
    pub fn to_sys(&self) -> sys::godot_property_hint {
        match *self {
            PropertyHint::None => GODOT_PROPERTY_HINT_NONE,
            PropertyHint::Range { .. } => GODOT_PROPERTY_HINT_RANGE,
            PropertyHint::Enum { .. } => GODOT_PROPERTY_HINT_ENUM,
            PropertyHint::Flags { .. } => GODOT_PROPERTY_HINT_FLAGS,
            PropertyHint::NodePathToEditedNode => GODOT_PROPERTY_HINT_NODE_PATH_TO_EDITED_NODE,
        }
    }
}

bitflags! {
    pub struct PropertyUsage: u32 {
        const STORAGE = GODOT_PROPERTY_USAGE_STORAGE as u32;
        const EDITOR = GODOT_PROPERTY_USAGE_EDITOR as u32;
        const NETWORK = GODOT_PROPERTY_USAGE_NETWORK as u32;
        const EDITOR_HELPER = GODOT_PROPERTY_USAGE_EDITOR_HELPER as u32;
        const CHECKABLE = GODOT_PROPERTY_USAGE_CHECKABLE as u32;
        const CHECKED = GODOT_PROPERTY_USAGE_CHECKED as u32;
        const INTERNATIONALIZED = GODOT_PROPERTY_USAGE_INTERNATIONALIZED as u32;
        const GROUP = GODOT_PROPERTY_USAGE_GROUP as u32;
        const CATEGORY = GODOT_PROPERTY_USAGE_CATEGORY as u32;
        const STORE_IF_NONZERO = GODOT_PROPERTY_USAGE_STORE_IF_NONZERO as u32;
        const STORE_IF_NONONE = GODOT_PROPERTY_USAGE_STORE_IF_NONONE as u32;
        const NO_INSTANCE_STATE = GODOT_PROPERTY_USAGE_NO_INSTANCE_STATE as u32;
        const RESTART_IF_CHANGED = GODOT_PROPERTY_USAGE_RESTART_IF_CHANGED as u32;
        const SCRIPT_VARIABLE  = GODOT_PROPERTY_USAGE_SCRIPT_VARIABLE as u32;
        const STORE_IF_NULL = GODOT_PROPERTY_USAGE_STORE_IF_NULL as u32;
        const ANIMATE_AS_TRIGGER = GODOT_PROPERTY_USAGE_ANIMATE_AS_TRIGGER as u32;
        const UPDATE_ALL_IF_MODIFIED = GODOT_PROPERTY_USAGE_UPDATE_ALL_IF_MODIFIED as u32;

        const DEFAULT = Self::STORAGE.bits | Self::EDITOR.bits | Self::NETWORK.bits;
        const DEFAULT_INTL = Self::DEFAULT.bits | Self::INTERNATIONALIZED.bits;
        const NOEDITOR = Self::STORAGE.bits | Self::NETWORK.bits;
    }
}

impl PropertyUsage {
    pub fn to_sys(&self) -> sys::godot_property_usage_flags {
        unsafe { mem::transmute(*self) }
    }
}

pub struct Property<'l, T, S, G>
{
    pub name: &'l str,
    pub setter: S,
    pub getter: G,
    pub default: T,
    pub hint: PropertyHint<'l>,
    pub usage: PropertyUsage,
}

// TODO: Signal arguments.

//pub struct SignalArgument<'l> {
//    pub name: &'str,
//    pub default: Variant,
//    pub hint: PropertyHint,
//    pub usage: PropertyUsage,
//}

pub struct Signal<'l> {
    pub name: &'l str,
    //pub args: &'l [SignalArgument],
}

pub unsafe trait PropertySetter<C: NativeClass, T: GodotType> {
    unsafe fn as_godot_function(self) -> sys::godot_property_set_func;
}

pub unsafe trait PropertyGetter<C: NativeClass, T: GodotType> {
    unsafe fn as_godot_function(self) -> sys::godot_property_get_func;
}

extern "C" fn empty_setter(
    _this: *mut sys::godot_object,
    _method: *mut libc::c_void,
    _class: *mut libc::c_void,
    _val: *mut sys::godot_variant
) {}

extern "C" fn empty_getter(
    _this: *mut sys::godot_object,
    _method: *mut libc::c_void,
    _class: *mut libc::c_void
) -> sys::godot_variant {
    Variant::new().forget()
}

extern "C" fn empty_free_func(_data: *mut libc::c_void) {}

unsafe impl <C: NativeClass, T: GodotType> PropertySetter<C, T> for () {
    unsafe fn as_godot_function(self) -> sys::godot_property_set_func {
        let mut set = sys::godot_property_set_func::default();
        set.set_func = Some(empty_setter);
        set.free_func = Some(empty_free_func);
        set
    }
}

unsafe impl <C: NativeClass, T: GodotType> PropertyGetter<C, T> for () {
    unsafe fn as_godot_function(self) -> sys::godot_property_get_func {
        let mut get = sys::godot_property_get_func::default();
        get.get_func = Some(empty_getter);
        get.free_func = Some(empty_free_func);
        get
    }
}

unsafe impl <F, C, T> PropertySetter<C, T> for F
    where C: NativeClass,
          T: GodotType,
          F: Fn(&mut C, T),
{
    unsafe fn as_godot_function(self) -> sys::godot_property_set_func {
        use std::cell::RefCell;
        let mut set = sys::godot_property_set_func::default();
        let data = Box::new(self);
        set.method_data = Box::into_raw(data) as *mut _;

        extern "C" fn invoke<C, F, T>(_this: *mut sys::godot_object, method: *mut libc::c_void, class: *mut libc::c_void, val: *mut sys::godot_variant)
            where C: NativeClass,
                T: GodotType,
                F: Fn(&mut C, T),

        {
            unsafe {
                let rust_ty = &*(class as *mut RefCell<C>);
                let mut rust_ty = rust_ty.borrow_mut();
                let func = &mut *(method as *mut F);

                if let Some(val) = T::from_variant(Variant::cast_ref(val)) {
                    func(&mut *rust_ty, val);
                } else {
                    godot_error!("Incorrect type passed to property");
                }
            }
        }
        set.set_func = Some(invoke::<C, F, T>);

        extern "C" fn free_func<F>(data: *mut libc::c_void) {
            unsafe {
                drop(Box::from_raw(data as *mut F));
            }
        }
        set.free_func = Some(free_func::<F>);

        set
    }
}

unsafe impl <F, C, T> PropertyGetter<C, T> for F
    where C: NativeClass,
          T: GodotType,
          F: Fn(&mut C) -> T,
{
    unsafe fn as_godot_function(self) -> sys::godot_property_get_func {
        use std::cell::RefCell;
        let mut get = sys::godot_property_get_func::default();
        let data = Box::new(self);
        get.method_data = Box::into_raw(data) as *mut _;

        extern "C" fn invoke<C, F, T>(_this: *mut sys::godot_object, method: *mut libc::c_void, class: *mut libc::c_void) -> sys::godot_variant
            where C: NativeClass,
                T: GodotType,
                F: Fn(&mut C) -> T,

        {
            unsafe {
                let rust_ty = &*(class as *mut RefCell<C>);
                let mut rust_ty = rust_ty.borrow_mut();
                let func = &mut *(method as *mut F);
                let ret = func(&mut *rust_ty);
                ret.to_variant().forget()
            }
        }
        get.get_func = Some(invoke::<C, F, T>);

        extern "C" fn free_func<F>(data: *mut libc::c_void) {
            unsafe {
                drop(Box::from_raw(data as *mut F));
            }
        }
        get.free_func = Some(free_func::<F>);

        get
    }
}
