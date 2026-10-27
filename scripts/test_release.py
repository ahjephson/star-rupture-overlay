"""Archive guard and deployment regressions using disposable directories only."""
import hashlib
import json
import pathlib
import shutil
import subprocess
import sys
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[1]


def powershell(script, *args, succeeds=True):
    result = subprocess.run(["pwsh", "-NoProfile", "-File", str(ROOT / "scripts" / script), *map(str, args)], capture_output=True, text=True, encoding="utf-8", timeout=60)
    assert (result.returncode == 0) == succeeds, result.stdout + result.stderr
    return result.stdout


def checksum(path):
    path.with_suffix(path.suffix + ".sha256").write_text(hashlib.sha256(path.read_bytes()).hexdigest() + "  " + path.name + "\n", encoding="ascii")


def fixture(source, path, change):
    with zipfile.ZipFile(source) as original:
        files = {name: original.read(name) for name in original.namelist()}
    change(files)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, data in files.items():
            archive.writestr(name, data)
    checksum(path)
    return path


def main(archive, work):
    new_name = "starrupture-overlay.exe"
    old_name = "starrupture-wave-overlay.exe"
    fresh = work / "fresh-installation"
    powershell("Deploy.ps1", "-ArchivePath", archive, "-Destination", fresh)
    fresh_state = json.loads((fresh / "installation.json").read_text(encoding="utf-8-sig"))
    assert (fresh / fresh_state["current"] / new_name).is_file()
    def legacy_name(files):
        files[old_name] = files.pop(new_name)
        manifest = json.loads(files["manifest.json"])
        manifest["files"][old_name] = manifest["files"].pop(new_name)
        manifest.pop("executable", None)
        files["manifest.json"] = json.dumps(manifest).encode()
    legacy = fixture(archive, work / "legacy.zip", legacy_name)
    deployed = work / "installation"
    powershell("Deploy.ps1", "-ArchivePath", legacy, "-Destination", deployed)
    first = json.loads((deployed / "installation.json").read_text(encoding="utf-8-sig"))
    assert (deployed / first["current"] / old_name).is_file()
    # Same build repackaged with a synthetic note exercises a distinct version folder.
    def next_version(files):
        files["README.md"] += b"\nSynthetic deployment regression fixture.\n"
        manifest = json.loads(files["manifest.json"])
        manifest["files"]["README.md"] = {"bytes": len(files["README.md"]), "sha256": hashlib.sha256(files["README.md"]).hexdigest()}
        files["manifest.json"] = json.dumps(manifest).encode()
    second = fixture(archive, work / "second.zip", next_version)
    powershell("Deploy.ps1", "-ArchivePath", second, "-Destination", deployed)
    state = json.loads((deployed / "installation.json").read_text(encoding="utf-8-sig"))
    assert state["previous"] == first["current"] and state["current"] != state["previous"]
    assert (deployed / state["current"] / new_name).is_file()
    powershell("Deploy.ps1", "-Rollback", "-Destination", deployed)
    restored = json.loads((deployed / "installation.json").read_text(encoding="utf-8-sig"))
    assert restored["current"] == first["current"]
    assert (deployed / restored["current"] / old_name).is_file()
    # Each malformed package must fail without switching the installed selection.
    def both_executables(files):
        files[old_name] = files[new_name]
        manifest = json.loads(files["manifest.json"])
        manifest["files"][old_name] = manifest["files"][new_name]
        files["manifest.json"] = json.dumps(manifest).encode()
    def wrong_executable(files):
        manifest = json.loads(files["manifest.json"])
        manifest["executable"] = old_name
        files["manifest.json"] = json.dumps(manifest).encode()
    cases = {
        "two-executables": both_executables,
        "wrong-executable": wrong_executable,
        "traversal": lambda files: files.update({"../escaped.txt": b"escape"}),
        "tampered": lambda files: files.update({"starrupture-overlay.exe": b"tampered"}),
        "extra": lambda files: files.update({"licenses/extra/manifest.json": b"{}"}),
        "duplicate-case": lambda files: files.update({"license": files["LICENSE"]}),
        "rooted": lambda files: files.update({"/escaped.txt": b"escape"}),
    }
    for name, change in cases.items():
        bad = fixture(archive, work / (name + ".zip"), change)
        powershell("Deploy.ps1", "-ArchivePath", bad, "-Destination", deployed, succeeds=False)
        assert json.loads((deployed / "installation.json").read_text(encoding="utf-8-sig")) == restored
    bad = work / "bad-checksum.zip"
    shutil.copy2(archive, bad)
    bad.with_suffix(".zip.sha256").write_text("0" * 64 + "  " + bad.name + "\n", encoding="ascii")
    powershell("Deploy.ps1", "-ArchivePath", bad, "-Destination", deployed, succeeds=False)
    assert not (work / "escaped.txt").exists()
    assert not (deployed / "escaped.txt").exists()
    print(json.dumps({"status": "passed", "installUpgradeRollback": True, "legacyNameUpgradeRollback": True, "rejectedArchiveCases": list(cases) + ["bad-checksum"], "appLaunched": False}))


if __name__ == "__main__":
    main(pathlib.Path(sys.argv[1]).resolve(), pathlib.Path(sys.argv[2]).resolve())
