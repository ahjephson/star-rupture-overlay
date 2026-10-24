"""Inspect a built executable without loading or executing its code."""
import json
import pathlib
import struct
import sys


def inspect(path):
    data = pathlib.Path(path).read_bytes()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    assert data[:2] == b"MZ" and data[pe:pe + 4] == b"PE\0\0", "Invalid PE file"
    machine, sections = struct.unpack_from("<HH", data, pe + 4)
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    assert machine == 0x8664, "Release must be Windows x64"
    assert struct.unpack_from("<H", data, optional)[0] == 0x20B, "Expected PE32+"
    assert struct.unpack_from("<H", data, optional + 68)[0] == 2, "Expected GUI subsystem"
    headers = optional + optional_size

    def offset(rva):
        for index in range(sections):
            section = headers + index * 40
            virtual_size, virtual, raw_size, raw = struct.unpack_from("<IIII", data, section + 8)
            if virtual <= rva < virtual + max(virtual_size, raw_size):
                result = raw + rva - virtual
                assert result < len(data), "Invalid section address"
                return result
        raise AssertionError("Unmapped PE address")

    import_rva = struct.unpack_from("<I", data, optional + 120)[0]
    imports = []
    if import_rva:
        descriptor = offset(import_rva)
        while True:
            values = struct.unpack_from("<IIIII", data, descriptor)
            if not any(values):
                break
            start = offset(values[3])
            imports.append(data[start:data.index(0, start)].decode("ascii"))
            descriptor += 20
    assert not any(name.lower().startswith(("vcruntime", "msvcp", "msvcr")) for name in imports), "VC runtime redistributable dependency"
    assert not any(marker in data for marker in (b"C:" + bytes([92]) + b"Users" + bytes([92]), b"C:" + bytes([47]) + b"Users" + bytes([47]))), "Personal build path embedded"
    return {"target": "windows-x64", "subsystem": "GUI", "bytes": len(data), "imports": imports, "separateVcRuntimeRequired": False}


if __name__ == "__main__":
    print(json.dumps(inspect(sys.argv[1]), indent=2))
