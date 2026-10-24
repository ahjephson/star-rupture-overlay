use crate::{
    containers::{FName, FString, TArray},
    mem::Ptr,
    read_path,
};
use anyhow::Result;
use jmap::{
    EClassCastFlags, EClassFlags, ECppForm, EEnumFlags, EFunctionFlags, EObjectFlags,
    EPropertyFlags, EStructFlags,
};

macro_rules! inherit {
    ($class:ident : UObject) => {
        impl Ptr<$class> {
            #[allow(unused)]
            pub fn uobject(&self) -> Ptr<UObject> {
                self.cast()
            }
            #[allow(unused)]
            pub async fn path(&self) -> Result<String> {
                self.uobject().path().await
            }
            #[allow(unused)]
            pub fn class_private(&self) -> Ptr<Ptr<UClass>> {
                self.uobject().class_private()
            }
        }
    };
    ($class:ident : UField) => {
        inherit!($class : UObject);
        impl Ptr<$class> {
            #[allow(unused)]
            pub fn ufield(&self) -> Ptr<UField> {
                self.cast()
            }
        }
    };
    ($class:ident : UStruct) => {
        inherit!($class : UObject);
        impl Ptr<$class> {
            #[allow(unused)]
            pub fn ustruct(&self) -> Ptr<UStruct> {
                self.cast()
            }
        }
    };

    ($class:ident : FField) => {
        impl Ptr<$class> {
            #[allow(unused)]
            pub fn ffield(&self) -> Ptr<FField> {
                self.cast()
            }
        }
    };
    ($class:ident : ZField) => {
        impl Ptr<$class> {
            #[allow(unused)]
            pub fn zfield(&self) -> Ptr<ZField> {
                self.cast()
            }
        }
    };
}

#[derive(Clone, Copy)]
pub struct FOutputDevice;
impl Ptr<FOutputDevice> {
    pub fn vtable(&self) -> Ptr<usize> {
        self.cast()
    }
    pub fn suppress_event_tag(&self) -> Ptr<bool> {
        let offset = self
            .ctx()
            .struct_member("FOutputDevice", "bSuppressEventTag");
        self.byte_offset(offset).cast()
    }
    pub fn auto_emit_line_terminator(&self) -> Ptr<bool> {
        let offset = self
            .ctx()
            .struct_member("FOutputDevice", "bAutoEmitLineTerminator");
        self.byte_offset(offset).cast()
    }
}

#[derive(Clone, Copy)]
pub struct FOutParmRec;
impl Ptr<FOutParmRec> {
    pub fn property(&self) -> Ptr<Ptr<ZProperty>> {
        let offset = self.ctx().struct_member("FOutParmRec", "Property");
        self.byte_offset(offset).cast()
    }
    pub fn prop_addr(&self) -> Ptr<u64> {
        let offset = self.ctx().struct_member("FOutParmRec", "PropAddr");
        self.byte_offset(offset).cast()
    }
    pub fn next_out_parm(&self) -> Ptr<Option<Ptr<FOutParmRec>>> {
        let offset = self.ctx().struct_member("FOutParmRec", "NextOutParm");
        self.byte_offset(offset).cast()
    }
}

#[derive(Clone, Copy)]
pub struct FFrame;
impl Ptr<FFrame> {
    pub fn foutput_device(&self) -> Ptr<FOutputDevice> {
        self.cast()
    }
    pub fn node(&self) -> Ptr<Ptr<UFunction>> {
        let offset = self.ctx().struct_member("FFrame", "Node");
        self.byte_offset(offset).cast()
    }
    pub fn object(&self) -> Ptr<Ptr<UObject>> {
        let offset = self.ctx().struct_member("FFrame", "Object");
        self.byte_offset(offset).cast()
    }
    pub fn code(&self) -> Ptr<u64> {
        let offset = self.ctx().struct_member("FFrame", "Code");
        self.byte_offset(offset).cast()
    }
    pub fn locals(&self) -> Ptr<u64> {
        let offset = self.ctx().struct_member("FFrame", "Locals");
        self.byte_offset(offset).cast()
    }
    pub fn most_recent_property(&self) -> Ptr<Option<Ptr<ZProperty>>> {
        let offset = self.ctx().struct_member("FFrame", "MostRecentProperty");
        self.byte_offset(offset).cast()
    }
    pub fn most_recent_property_address(&self) -> Ptr<u64> {
        let offset = self
            .ctx()
            .struct_member("FFrame", "MostRecentPropertyAddress");
        self.byte_offset(offset).cast()
    }
    pub fn most_recent_property_container(&self) -> Ptr<u64> {
        let offset = self
            .ctx()
            .struct_member("FFrame", "MostRecentPropertyContainer");
        self.byte_offset(offset).cast()
    }
    pub fn previous_frame(&self) -> Ptr<Option<Ptr<FFrame>>> {
        let offset = self.ctx().struct_member("FFrame", "PreviousFrame");
        self.byte_offset(offset).cast()
    }
    pub fn out_parms(&self) -> Ptr<Option<Ptr<FOutParmRec>>> {
        let offset = self.ctx().struct_member("FFrame", "OutParms");
        self.byte_offset(offset).cast()
    }
    pub fn property_chain_for_compiled_in(&self) -> Ptr<Option<Ptr<ZField>>> {
        let offset = self
            .ctx()
            .struct_member("FFrame", "PropertyChainForCompiledIn");
        self.byte_offset(offset).cast()
    }
    pub fn current_native_function(&self) -> Ptr<Option<Ptr<UFunction>>> {
        let offset = self.ctx().struct_member("FFrame", "CurrentNativeFunction");
        self.byte_offset(offset).cast()
    }
    pub fn previous_tracking_frame(&self) -> Ptr<Option<Ptr<FFrame>>> {
        let offset = self.ctx().struct_member("FFrame", "PreviousTrackingFrame");
        self.byte_offset(offset).cast()
    }
    pub fn array_context_failed(&self) -> Ptr<bool> {
        let offset = self.ctx().struct_member("FFrame", "bArrayContextFailed");
        self.byte_offset(offset).cast()
    }
    pub fn aborting_execution(&self) -> Ptr<bool> {
        let offset = self.ctx().struct_member("FFrame", "bAbortingExecution");
        self.byte_offset(offset).cast()
    }
}

#[derive(Clone, Copy)]
pub struct UObject;
impl Ptr<UObject> {
    pub fn vtable(&self) -> Ptr<usize> {
        self.cast()
    }
    pub fn object_flags(&self) -> Ptr<EObjectFlags> {
        let offset = self.ctx().struct_member("UObject", "ObjectFlags");
        self.byte_offset(offset).cast()
    }
    pub fn internal_index(&self) -> Ptr<i32> {
        let offset = self.ctx().struct_member("UObject", "InternalIndex");
        self.byte_offset(offset).cast()
    }
    pub fn class_private(&self) -> Ptr<Ptr<UClass>> {
        let offset = self.ctx().struct_member("UObject", "ClassPrivate");
        self.byte_offset(offset).cast()
    }
    pub fn name_private(&self) -> Ptr<FName> {
        let offset = self.ctx().struct_member("UObject", "NamePrivate");
        self.byte_offset(offset).cast()
    }
    pub fn outer_private(&self) -> Ptr<Option<Ptr<UObject>>> {
        let offset = self.ctx().struct_member("UObject", "OuterPrivate");
        self.byte_offset(offset).cast()
    }
    pub async fn path(&self) -> Result<String> {
        read_path(self).await
    }
}

#[derive(Clone, Copy)]
pub struct UField;
inherit!(UField : UObject);
impl Ptr<UField> {
    pub fn next(&self) -> Ptr<Option<Ptr<UField>>> {
        let offset = self.ctx().struct_member("UField", "Next");
        self.byte_offset(offset).cast()
    }
}

#[derive(Clone, Copy)]
pub struct UStruct;
inherit!(UStruct : UField);
impl Ptr<UStruct> {
    pub fn super_struct(&self) -> Ptr<Option<Ptr<UStruct>>> {
        let offset = self.ctx().struct_member("UStruct", "SuperStruct");
        self.byte_offset(offset).cast()
    }
    pub fn children(&self) -> Ptr<Option<Ptr<UField>>> {
        let offset = self.ctx().struct_member("UStruct", "Children");
        self.byte_offset(offset).cast()
    }
    pub fn child_properties(&self) -> Ptr<Option<Ptr<ZField>>> {
        let offset = self.ctx().struct_member("UStruct", "ChildProperties");
        self.byte_offset(offset).cast()
    }
    pub fn properties_size(&self) -> Ptr<i32> {
        let offset = self.ctx().struct_member("UStruct", "PropertiesSize");
        self.byte_offset(offset).cast()
    }
    pub fn min_alignment(&self) -> Ptr<i32> {
        let offset = self.ctx().struct_member("UStruct", "MinAlignment");
        self.byte_offset(offset).cast()
    }
    pub fn script(&self) -> Ptr<TArray<u8>> {
        let offset = self.ctx().struct_member("UStruct", "Script");
        self.byte_offset(offset).cast()
    }
    pub fn properties(&self, recurse_parents: bool) -> PropertyIterator {
        PropertyIterator {
            current_struct: Some(self.clone()),
            current_field: None,
            recurse_parents,
        }
    }
    pub fn child_fields(&self) -> Ptr<Option<Ptr<ZField>>> {
        if self.ctx().ue_version() < (4, 25) {
            self.children().cast()
        } else {
            self.child_properties()
        }
    }
}

#[derive(Clone, Copy)]
pub struct UClass;
inherit!(UClass : UStruct);
impl Ptr<UClass> {
    pub fn class_flags(&self) -> Ptr<EClassFlags> {
        let offset = self.ctx().struct_member("UClass", "ClassFlags");
        self.byte_offset(offset).cast()
    }
    pub fn class_cast_flags(&self) -> Ptr<EClassCastFlags> {
        let offset = self.ctx().struct_member("UClass", "ClassCastFlags");
        self.byte_offset(offset).cast()
    }
    pub fn class_default_object(&self) -> Ptr<Option<Ptr<UObject>>> {
        let offset = self.ctx().struct_member("UClass", "ClassDefaultObject");
        self.byte_offset(offset).cast()
    }
    pub fn interfaces(&self) -> Ptr<TArray<()>> {
        let offset = self.ctx().struct_member("UClass", "Interfaces");
        self.byte_offset(offset).cast()
    }
    pub async fn read_interfaces(&self) -> Result<Vec<(Option<Ptr<UClass>>, i32, bool)>> {
        let len = self.interfaces().len().await?;
        if len == 0 {
            return Ok(vec![]);
        }
        let Some(data) = self.interfaces().data().await? else {
            return Ok(vec![]);
        };
        let data: Ptr<FImplementedInterface> = data.cast();
        let size = self.ctx().get_struct("FImplementedInterface").size as usize;

        let mut interfaces = Vec::with_capacity(len);
        for i in 0..len {
            let elm = data.byte_offset(i * size);
            interfaces.push((
                elm.class().read().await?,
                elm.pointer_offset().read().await?,
                elm.implemented_by_k2().read().await? != 0,
            ));
        }
        Ok(interfaces)
    }
}

#[derive(Clone, Copy)]
pub struct FImplementedInterface;
impl Ptr<FImplementedInterface> {
    pub fn class(&self) -> Ptr<Option<Ptr<UClass>>> {
        let offset = self.ctx().struct_member("FImplementedInterface", "Class");
        self.byte_offset(offset).cast()
    }
    pub fn pointer_offset(&self) -> Ptr<i32> {
        let offset = self
            .ctx()
            .struct_member("FImplementedInterface", "PointerOffset");
        self.byte_offset(offset).cast()
    }
    pub fn implemented_by_k2(&self) -> Ptr<u8> {
        let offset = self
            .ctx()
            .struct_member("FImplementedInterface", "bImplementedByK2");
        self.byte_offset(offset).cast()
    }
}

#[derive(Clone, Copy)]
pub struct UScriptStruct;
inherit!(UScriptStruct : UStruct);
impl Ptr<UScriptStruct> {
    pub fn struct_flags(&self) -> Ptr<EStructFlags> {
        let offset = self.ctx().struct_member("UScriptStruct", "StructFlags");
        self.byte_offset(offset).cast()
    }
}

#[derive(Clone, Copy)]
pub struct UFunction;
inherit!(UFunction : UStruct);
impl Ptr<UFunction> {
    pub fn function_flags(&self) -> Ptr<EFunctionFlags> {
        let offset = self.ctx().struct_member("UFunction", "FunctionFlags");
        self.byte_offset(offset).cast()
    }
    pub fn func(&self) -> Ptr<usize> {
        let offset = self.ctx().struct_member("UFunction", "Func");
        self.byte_offset(offset).cast()
    }
}

#[derive(Clone, Copy)]
pub struct UEnum;
inherit!(UEnum : UField);
impl Ptr<UEnum> {
    pub fn cpp_type(&self) -> Ptr<FString> {
        let offset = self.ctx().struct_member("UEnum", "CppType");
        self.byte_offset(offset).cast()
    }
    /// size of element depends on version so up to caller to figure that out
    pub fn names(&self) -> Ptr<TArray<()>> {
        let offset = self.ctx().struct_member("UEnum", "Names");
        self.byte_offset(offset).cast()
    }
    pub fn cpp_form(&self) -> Ptr<ECppForm> {
        let offset = self.ctx().struct_member("UEnum", "CppForm");
        self.byte_offset(offset).cast()
    }
    pub fn enum_flags(&self) -> Ptr<EEnumFlags> {
        let offset = self.ctx().struct_member("UEnum", "EnumFlags");
        self.byte_offset(offset).cast()
    }
}
#[derive(Clone, Copy)]
pub struct UEnumNameTuple;
impl Ptr<UEnumNameTuple> {
    pub fn name(&self) -> Ptr<FName> {
        let offset = self.ctx().struct_member("UEnumNameTuple", "Name");
        self.byte_offset(offset).cast()
    }
    pub fn value(&self) -> Ptr<()> {
        let offset = self.ctx().struct_member("UEnumNameTuple", "Value");
        self.byte_offset(offset).cast()
    }
}
#[derive(Clone, Copy)]
pub struct FNameData;
impl Ptr<FNameData> {
    pub fn tagged_names(&self) -> Ptr<u64> {
        let offset = self.ctx().struct_member("FNameData", "TaggedNames");
        self.byte_offset(offset).cast()
    }
    pub fn tagged_values(&self) -> Ptr<u64> {
        let offset = self.ctx().struct_member("FNameData", "TaggedValues");
        self.byte_offset(offset).cast()
    }
    pub fn num_values(&self) -> Ptr<i32> {
        let offset = self.ctx().struct_member("FNameData", "NumValues");
        self.byte_offset(offset).cast()
    }
}
impl Ptr<UEnum> {
    pub async fn read_names(&self) -> Result<Vec<(String, i64)>> {
        let version = self.ctx().ue_version();
        let uenum = self.ctx().get_struct("UEnum");
        let off = |n: &str| uenum.members.iter().find(|m| m.name == n).map(|m| m.offset);
        let names_size = match (off("Names"), off("CppForm")) {
            (Some(a), Some(b)) => b - a,
            _ => 0,
        };

        if names_size >= 24 {
            // FNameData
            let name_data: Ptr<FNameData> = self.names().cast();
            let len = name_data.num_values().read().await? as usize;
            if len == 0 {
                return Ok(vec![]);
            }

            let tagged_names = name_data.tagged_names().read().await?;
            let tagged_values = name_data.tagged_values().read().await?;
            let names_addr = tagged_names & !1u64;
            let values_addr = tagged_values & !1u64;

            let fname_size = self.ctx().get_struct("FName").size as usize;

            let mut names = vec![];
            for i in 0..len {
                let name_ptr: Ptr<FName> =
                    Ptr::new(names_addr + (i * fname_size) as u64, self.ctx().clone())?;
                let name = name_ptr.read().await?;

                let value_ptr: Ptr<i64> =
                    Ptr::new(values_addr + (i * 8) as u64, self.ctx().clone())?;
                let value = value_ptr.read().await?;

                names.push((name, value));
            }
            Ok(names)
        } else {
            // Pre-5.7: TArray<UEnumNameTuple>
            let mut names = vec![];
            let len = self.names().len().await?;
            if len > 0 {
                let data: Ptr<UEnumNameTuple> = self.names().data().await?.unwrap().cast();
                let size = self.ctx().get_struct("UEnumNameTuple").size;
                for i in 0..len {
                    let elm = data.byte_offset(i * size as usize);
                    let name = elm.name().read().await?;
                    let value = if version < (4, 9) {
                        i as i64
                    } else if version < (4, 15) {
                        elm.value().cast::<u8>().read().await? as i64
                    } else {
                        elm.value().cast::<i64>().read().await?
                    };
                    names.push((name, value));
                }
            }
            Ok(names)
        }
    }
}

#[derive(Clone, Copy)]
pub struct ZField;
impl Ptr<ZField> {
    pub fn next(&self) -> Ptr<Option<Ptr<ZField>>> {
        let offset = self.ctx().struct_member("ZField", "Next");
        self.byte_offset(offset).cast()
    }
    pub fn name_private(&self) -> Ptr<FName> {
        let offset = self.ctx().struct_member("ZField", "NamePrivate");
        self.byte_offset(offset).cast()
    }
    pub async fn cast_flags(&self) -> Result<EClassCastFlags> {
        if self.ctx().ue_version() < (4, 25) {
            // UField
            let class = self.cast::<UObject>().class_private().read().await?;
            class.class_cast_flags().read().await
        } else {
            // FField
            let offset = self.ctx().struct_member("FField", "ClassPrivate");
            let class: Ptr<Ptr<FFieldClass>> = self.byte_offset(offset).cast();
            class.read().await?.cast_flags().read().await
        }
    }
    pub async fn owner(&self) -> Result<Option<FieldOwner>> {
        if self.ctx().ue_version() < (4, 25) {
            let Some(outer) = self.cast::<UObject>().outer_private().read().await? else {
                return Ok(None);
            };
            let is_prop = outer
                .cast::<ZField>()
                .cast_flags()
                .await?
                .contains(EClassCastFlags::CASTCLASS_FProperty);
            return Ok(Some(if is_prop {
                FieldOwner::Field(outer.cast())
            } else {
                FieldOwner::Object(outer)
            }));
        }

        let owner_off = self.ctx().struct_member("FField", "Owner");
        let variant: Ptr<FFieldVariant> = self.byte_offset(owner_off).cast();
        variant.read().await
    }
}

#[derive(Clone, Copy)]
pub struct FFieldVariant;
impl Ptr<FFieldVariant> {
    pub fn container(&self) -> Ptr<u64> {
        let offset = self.ctx().struct_member("FFieldVariant", "Container");
        self.byte_offset(offset).cast()
    }
    pub fn is_uobject(&self) -> Ptr<bool> {
        let offset = self.ctx().struct_member("FFieldVariant", "bIsUObject");
        self.byte_offset(offset).cast()
    }
    pub async fn read(&self) -> Result<Option<FieldOwner>> {
        let container = self.container().read().await?;
        if container == 0 {
            return Ok(None);
        }
        let ctx = self.ctx().clone();
        let (is_uobject, ptr) = if self.ctx().ue_version() >= (5, 3) {
            (container & 0x1 != 0, container & !0x1)
        } else {
            (self.is_uobject().read().await?, container)
        };
        Ok(Some(if is_uobject {
            FieldOwner::Object(Ptr::<UObject>::new(ptr, ctx)?)
        } else {
            FieldOwner::Field(Ptr::<ZField>::new(ptr, ctx)?)
        }))
    }
}

pub enum FieldOwner {
    Object(Ptr<UObject>),
    Field(Ptr<ZField>),
}

#[derive(Clone, Copy)]
pub struct FFieldClass;
impl Ptr<FFieldClass> {
    pub fn cast_flags(&self) -> Ptr<EClassCastFlags> {
        let offset = self.ctx().struct_member("FFieldClass", "CastFlags");
        self.byte_offset(offset).cast()
    }
}

#[derive(Clone, Copy)]
pub struct ZProperty;
inherit!(ZProperty : ZField);
impl Ptr<ZProperty> {
    pub fn array_dim(&self) -> Ptr<i32> {
        let offset = self.ctx().struct_member("ZProperty", "ArrayDim");
        self.byte_offset(offset).cast()
    }
    pub fn element_size(&self) -> Ptr<i32> {
        let offset = self.ctx().struct_member("ZProperty", "ElementSize");
        self.byte_offset(offset).cast()
    }
    pub fn property_flags(&self) -> Ptr<EPropertyFlags> {
        let offset = self.ctx().struct_member("ZProperty", "PropertyFlags");
        self.byte_offset(offset).cast()
    }
    pub fn offset_internal(&self) -> Ptr<i32> {
        let offset = self.ctx().struct_member("ZProperty", "Offset_Internal");
        self.byte_offset(offset).cast()
    }
}

#[derive(Clone, Copy)]
pub struct ZBoolProperty;
impl Ptr<ZBoolProperty> {
    pub fn field_size(&self) -> Ptr<u8> {
        let offset = self.ctx().struct_member("ZBoolProperty", "FieldSize");
        self.byte_offset(offset).cast()
    }
    pub fn byte_offset_(&self) -> Ptr<u8> {
        let offset = self.ctx().struct_member("ZBoolProperty", "ByteOffset");
        self.byte_offset(offset).cast()
    }
    pub fn byte_mask(&self) -> Ptr<u8> {
        let offset = self.ctx().struct_member("ZBoolProperty", "ByteMask");
        self.byte_offset(offset).cast()
    }
    pub fn field_mask(&self) -> Ptr<u8> {
        let offset = self.ctx().struct_member("ZBoolProperty", "FieldMask");
        self.byte_offset(offset).cast()
    }
}
#[derive(Clone, Copy)]
pub struct ZObjectProperty;
impl Ptr<ZObjectProperty> {
    pub fn property_class(&self) -> Ptr<Ptr<UClass>> {
        let offset = self
            .ctx()
            .struct_member("ZObjectPropertyBase", "PropertyClass");
        self.byte_offset(offset).cast()
    }
}
#[derive(Clone, Copy)]
pub struct ZClassProperty;
impl Ptr<ZClassProperty> {
    pub fn fobject_property(&self) -> Ptr<ZObjectProperty> {
        self.cast()
    }
    pub fn meta_class(&self) -> Ptr<Ptr<UClass>> {
        let offset = self.ctx().struct_member("ZClassProperty", "MetaClass");
        self.byte_offset(offset).cast()
    }
}
#[derive(Clone, Copy)]
pub struct ZSoftObjectProperty;
impl Ptr<ZSoftObjectProperty> {
    pub fn property_class(&self) -> Ptr<Ptr<UClass>> {
        let offset = self
            .ctx()
            .struct_member("ZObjectPropertyBase", "PropertyClass");
        self.byte_offset(offset).cast()
    }
}
#[derive(Clone, Copy)]
pub struct ZSoftClassProperty;
impl Ptr<ZSoftClassProperty> {
    pub fn fsoft_object_property(&self) -> Ptr<ZSoftObjectProperty> {
        self.cast()
    }
    pub fn meta_class(&self) -> Ptr<Ptr<UClass>> {
        let offset = self.ctx().struct_member("ZSoftClassProperty", "MetaClass");
        self.byte_offset(offset).cast()
    }
}
#[derive(Clone, Copy)]
pub struct ZWeakObjectProperty;
impl Ptr<ZWeakObjectProperty> {
    pub fn property_class(&self) -> Ptr<Ptr<UClass>> {
        let offset = self
            .ctx()
            .struct_member("ZObjectPropertyBase", "PropertyClass");
        self.byte_offset(offset).cast()
    }
}
#[derive(Clone, Copy)]
pub struct ZLazyObjectProperty;
impl Ptr<ZLazyObjectProperty> {
    pub fn property_class(&self) -> Ptr<Ptr<UClass>> {
        let offset = self
            .ctx()
            .struct_member("ZObjectPropertyBase", "PropertyClass");
        self.byte_offset(offset).cast()
    }
}
#[derive(Clone, Copy)]
pub struct ZInterfaceProperty;
impl Ptr<ZInterfaceProperty> {
    pub fn interface_class(&self) -> Ptr<Ptr<UClass>> {
        let offset = self
            .ctx()
            .struct_member("ZInterfaceProperty", "InterfaceClass");
        self.byte_offset(offset).cast()
    }
}
#[derive(Clone, Copy)]
pub struct ZArrayProperty;
impl Ptr<ZArrayProperty> {
    pub fn inner(&self) -> Ptr<Ptr<ZProperty>> {
        let offset = self.ctx().struct_member("ZArrayProperty", "Inner");
        self.byte_offset(offset).cast()
    }
}
#[derive(Clone, Copy)]
pub struct ZStructProperty;
impl Ptr<ZStructProperty> {
    pub fn struct_(&self) -> Ptr<Ptr<UScriptStruct>> {
        let offset = self.ctx().struct_member("ZStructProperty", "Struct");
        self.byte_offset(offset).cast()
    }
}
#[derive(Clone, Copy)]
pub struct ZMapProperty;
impl Ptr<ZMapProperty> {
    pub fn key_prop(&self) -> Ptr<Ptr<ZProperty>> {
        let offset = self.ctx().struct_member("ZMapProperty", "KeyProp");
        self.byte_offset(offset).cast()
    }
    pub fn value_prop(&self) -> Ptr<Ptr<ZProperty>> {
        let offset = self.ctx().struct_member("ZMapProperty", "ValueProp");
        self.byte_offset(offset).cast()
    }
    pub fn map_layout(&self) -> Ptr<FScriptMapLayout> {
        let offset = self.ctx().struct_member("ZMapProperty", "MapLayout");
        self.byte_offset(offset).cast()
    }
}
#[derive(Clone, Copy)]
pub struct ZSetProperty;
impl Ptr<ZSetProperty> {
    pub fn element_prop(&self) -> Ptr<Ptr<ZProperty>> {
        let offset = self.ctx().struct_member("ZSetProperty", "ElementProp");
        self.byte_offset(offset).cast()
    }
    pub fn set_layout(&self) -> Ptr<FScriptSetLayout> {
        let offset = self.ctx().struct_member("ZSetProperty", "SetLayout");
        self.byte_offset(offset).cast()
    }
}

#[derive(Clone, Copy)]
pub struct FScriptSetLayout;
impl Ptr<FScriptSetLayout> {
    pub fn size(&self) -> Ptr<i32> {
        let offset = self.ctx().struct_member("FScriptSetLayout", "Size");
        self.byte_offset(offset).cast()
    }
}

#[derive(Clone, Copy)]
pub struct FScriptMapLayout;
impl Ptr<FScriptMapLayout> {
    pub fn value_offset(&self) -> Ptr<i32> {
        let offset = self.ctx().struct_member("FScriptMapLayout", "ValueOffset");
        self.byte_offset(offset).cast()
    }
    pub fn set_layout(&self) -> Ptr<FScriptSetLayout> {
        let offset = self.ctx().struct_member("FScriptMapLayout", "SetLayout");
        self.byte_offset(offset).cast()
    }
}
#[derive(Clone, Copy)]
pub struct ZEnumProperty;
impl Ptr<ZEnumProperty> {
    pub fn underlying_prop(&self) -> Ptr<Ptr<ZProperty>> {
        let offset = self.ctx().struct_member("ZEnumProperty", "UnderlyingProp");
        self.byte_offset(offset).cast()
    }
    pub fn enum_(&self) -> Ptr<Option<Ptr<UEnum>>> {
        let offset = self.ctx().struct_member("ZEnumProperty", "Enum");
        self.byte_offset(offset).cast()
    }
}
#[derive(Clone, Copy)]
pub struct ZByteProperty;
impl Ptr<ZByteProperty> {
    pub fn enum_(&self) -> Ptr<Option<Ptr<UEnum>>> {
        let offset = self.ctx().struct_member("ZByteProperty", "Enum");
        self.byte_offset(offset).cast()
    }
}
#[derive(Clone, Copy)]
pub struct FOptionalProperty;
impl Ptr<FOptionalProperty> {
    pub fn value_property(&self) -> Ptr<Ptr<ZProperty>> {
        // TODO implement struct inheritence. for now calculate offset manually
        let parent = self.ctx().get_struct("ZProperty").size as usize;
        let offset = self
            .ctx()
            .struct_member("FOptionalPropertyLayout", "ValueProperty");
        self.byte_offset(parent + offset).cast()
    }
}

#[derive(Clone, Copy)]
pub struct ZDelegateProperty;
impl Ptr<ZDelegateProperty> {
    pub fn signature_function(&self) -> Ptr<Option<Ptr<UFunction>>> {
        let offset = self
            .ctx()
            .struct_member("ZDelegateProperty", "SignatureFunction");
        self.byte_offset(offset).cast()
    }
}
#[derive(Clone, Copy)]
pub struct ZMulticastDelegateProperty;
impl Ptr<ZMulticastDelegateProperty> {
    pub fn signature_function(&self) -> Ptr<Option<Ptr<UFunction>>> {
        let offset = self
            .ctx()
            .struct_member("ZMulticastDelegateProperty", "SignatureFunction");
        self.byte_offset(offset).cast()
    }
}

#[derive(Clone, Copy)]
pub struct FUObjectItem;
impl Ptr<FUObjectItem> {
    pub fn object(&self) -> Ptr<Option<Ptr<UObject>>> {
        let offset = self.ctx().struct_member("FUObjectItem", "Object");
        self.byte_offset(offset).cast()
    }
    /// Offset by n items using the runtime struct size
    pub fn offset_item(&self, n: usize) -> Self {
        let stride = self.ctx().get_struct("FUObjectItem").size as usize;
        self.byte_offset(n * stride)
    }
}

#[derive(Clone, Copy)]
pub struct FFixedUObjectArray;
impl Ptr<FFixedUObjectArray> {
    pub fn objects(&self) -> Ptr<Ptr<FUObjectItem>> {
        let offset = self.ctx().struct_member("FFixedUObjectArray", "Objects");
        self.byte_offset(offset).cast()
    }
    pub fn num_elements(&self) -> Ptr<i32> {
        let offset = self
            .ctx()
            .struct_member("FFixedUObjectArray", "NumElements");
        self.byte_offset(offset).cast()
    }
    pub async fn read_item_ptr(&self, item: usize) -> Result<Ptr<FUObjectItem>> {
        Ok(self.objects().read().await?.offset_item(item))
    }
}

#[derive(Clone, Copy)]
pub struct FChunkedFixedUObjectArray;
impl Ptr<FChunkedFixedUObjectArray> {
    pub fn objects(&self) -> Ptr<Ptr<Ptr<FUObjectItem>>> {
        let offset = self
            .ctx()
            .struct_member("FChunkedFixedUObjectArray", "Objects");
        self.byte_offset(offset).cast()
    }
    pub fn num_elements(&self) -> Ptr<i32> {
        let offset = self
            .ctx()
            .struct_member("FChunkedFixedUObjectArray", "NumElements");
        self.byte_offset(offset).cast()
    }
    pub fn max_elements(&self) -> Ptr<i32> {
        let offset = self
            .ctx()
            .struct_member("FChunkedFixedUObjectArray", "MaxElements");
        self.byte_offset(offset).cast()
    }
    pub fn max_chunks(&self) -> Ptr<i32> {
        let offset = self
            .ctx()
            .struct_member("FChunkedFixedUObjectArray", "MaxChunks");
        self.byte_offset(offset).cast()
    }
    pub async fn elements_per_chunk(&self) -> Result<usize> {
        let max_elements = self.max_elements().read().await?;
        let max_chunks = self.max_chunks().read().await?;
        Ok((max_elements / max_chunks) as usize)
    }
    pub async fn read_item_ptr(&self, item: usize) -> Result<Ptr<FUObjectItem>> {
        let max_per_chunk = self.elements_per_chunk().await?;
        let chunk_index = item / max_per_chunk;

        Ok(self
            .objects()
            .read()
            .await?
            .offset(chunk_index)
            .read()
            .await?
            .offset_item(item % max_per_chunk))
    }
}
#[derive(Clone, Copy)]
pub struct FUObjectArrayOld;
impl Ptr<FUObjectArrayOld> {
    pub fn chunks(&self) -> Ptr<Ptr<Option<Ptr<UObject>>>> {
        let offset = self.ctx().struct_member("FUObjectArrayOld", "Chunks");
        self.byte_offset(offset).cast()
    }
    pub fn num_elements(&self) -> Ptr<i32> {
        let offset = self.ctx().struct_member("FUObjectArrayOld", "NumElements");
        self.byte_offset(offset).cast()
    }
    pub async fn read_item_ptr(&self, item: usize) -> Result<Option<Ptr<UObject>>> {
        let max_per_chunk = 16 * 1024;
        let chunk_index = item / max_per_chunk;

        self.chunks()
            .offset(chunk_index)
            .read()
            .await?
            .offset(item % max_per_chunk)
            .read()
            .await
    }
}
#[derive(Clone, Copy)]
pub struct FUObjectArrayOlder;
impl Ptr<FUObjectArrayOlder> {
    pub fn data(&self) -> Ptr<Ptr<Option<Ptr<UObject>>>> {
        self.cast()
    }
    pub fn num_elements(&self) -> Ptr<i32> {
        let offset = self.ctx().struct_member("FUObjectArrayOlder", "ArrayNum");
        self.byte_offset(offset).cast()
    }
    pub async fn read_item_ptr(&self, item: usize) -> Result<Option<Ptr<UObject>>> {
        self.data().read().await?.offset(item).read().await
    }
}

#[derive(Clone, Copy)]
pub struct FUObjectArray;
impl Ptr<FUObjectArray> {
    fn obj_objects(&self) -> Ptr<()> {
        let offset = self.ctx().struct_member("FUObjectArray", "ObjObjects");
        self.byte_offset(offset).cast()
    }
    pub async fn read_item_ptr(&self, item: usize) -> Result<Option<Ptr<UObject>>> {
        if self.ctx().ue_version() < (4, 8) {
            self.obj_objects()
                .cast::<FUObjectArrayOlder>()
                .read_item_ptr(item)
                .await
        } else if self.ctx().ue_version() < (4, 11) {
            self.obj_objects()
                .cast::<FUObjectArrayOld>()
                .read_item_ptr(item)
                .await
        } else if self.ctx().ue_version() < (4, 20) {
            self.obj_objects()
                .cast::<FFixedUObjectArray>()
                .read_item_ptr(item)
                .await?
                .object()
                .read()
                .await
        } else {
            self.obj_objects()
                .cast::<FChunkedFixedUObjectArray>()
                .read_item_ptr(item)
                .await?
                .object()
                .read()
                .await
        }
    }
    pub async fn num_elements(&self) -> Result<i32> {
        if self.ctx().ue_version() < (4, 8) {
            self.obj_objects()
                .cast::<FUObjectArrayOlder>()
                .num_elements()
        } else if self.ctx().ue_version() < (4, 11) {
            self.obj_objects().cast::<FUObjectArrayOld>().num_elements()
        } else if self.ctx().ue_version() < (4, 20) {
            self.obj_objects()
                .cast::<FFixedUObjectArray>()
                .num_elements()
        } else {
            self.obj_objects()
                .cast::<FChunkedFixedUObjectArray>()
                .num_elements()
        }
        .read()
        .await
    }
}

#[derive(Clone)]
pub struct PropertyIterator {
    current_struct: Option<Ptr<UStruct>>,
    current_field: Option<Ptr<ZField>>,
    recurse_parents: bool,
}

impl PropertyIterator {
    pub async fn next(&mut self) -> Option<Result<Ptr<ZProperty>>> {
        loop {
            if let Some(current) = self.current_field.take() {
                let is_property = match current.cast_flags().await {
                    Ok(flags) if flags.contains(EClassCastFlags::CASTCLASS_FProperty) => true,
                    Ok(_) => false,
                    Err(e) => return Some(Err(e)),
                };

                self.current_field = match current.next().read().await {
                    Ok(next) => next,
                    Err(e) => return Some(Err(e)),
                };

                if is_property {
                    return Some(Ok(current.cast::<ZProperty>()));
                }
            } else if let Some(current) = self.current_struct.take() {
                self.current_field = match current.child_fields().read().await {
                    Ok(children) => children,
                    Err(e) => return Some(Err(e)),
                };

                if self.recurse_parents {
                    self.current_struct = match current.super_struct().read().await {
                        Ok(super_struct) => super_struct,
                        Err(e) => return Some(Err(e)),
                    };
                }
            } else {
                return None;
            }
        }
    }
}
