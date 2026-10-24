// Core type definitions for Unreal Engine

input int UE_VERSION;
input bool WITH_CASE_PRESERVING_NAME = false;
input bool UE_PACK_FUOBJECT_ITEM = false;
input bool UE_EDITOR = false;
input bool WITH_EDITOR = false;
input bool WITH_EDITORONLY_DATA = false;
input bool STATS = false;
input bool UE_FUOBJECT_FLAGS_REFCOUNT = false;

// Definitions for types with explicit sizes
type int8_t = char;
type uint8_t = unsigned char;
type int16_t = short int;
type uint16_t = unsigned short int;
type int32_t = int;
type uint32_t = unsigned int;
type int64_t = long long int;
type uint64_t = unsigned long long int;
type uintptr_t = uint64_t;
