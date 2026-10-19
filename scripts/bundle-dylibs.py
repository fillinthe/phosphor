#!/usr/bin/env python3
"""Copies every Homebrew dylib a Mac app's binary needs into Contents/Frameworks and points the
binary and libraries at the copies, so the app runs on Macs without Homebrew. With a library folder
(vendor/ffmpeg/lib from scripts/build-ffmpeg-mac.sh), @rpath libraries found there are copied too.
Usage: bundle-dylibs.py path/to/App.app [library-folder]"""
import os, shutil, subprocess, sys

app = sys.argv[1]
exe_dir = os.path.join(app, "Contents", "MacOS")
fw = os.path.join(app, "Contents", "Frameworks")
lib_dir = sys.argv[2] if len(sys.argv) > 2 else None
os.makedirs(fw, exist_ok=True)

def deps(path):
    """(name as linked, file to copy) for each library that has to travel with the app."""
    out = subprocess.run(["otool", "-L", path], capture_output=True, text=True, check=True).stdout
    for line in out.splitlines()[1:]:
        name = line.strip().split(" (")[0]
        if name.startswith(("/opt/homebrew/", "/usr/local/")):
            yield name, name
        elif lib_dir and name.startswith("@rpath/") and os.path.exists(os.path.join(lib_dir, name[7:])):
            yield name, os.path.join(lib_dir, name[7:])

def run(*args):
    subprocess.run(args, check=True, capture_output=True)

binaries = [os.path.join(exe_dir, f) for f in os.listdir(exe_dir)]
todo = list(binaries)
copied = {}
while todo:
    f = todo.pop()
    for dep, src in deps(f):
        base = os.path.basename(dep)
        target = os.path.join(fw, base)
        if base not in copied:
            shutil.copy2(os.path.realpath(src), target)
            os.chmod(target, 0o755)
            run("install_name_tool", "-id", f"@rpath/{base}", target)
            copied[base] = target
            todo.append(target)
        if dep != f"@rpath/{base}":
            run("install_name_tool", "-change", dep, f"@rpath/{base}", f)

for b in binaries:
    rpaths = subprocess.run(["otool", "-l", b], capture_output=True, text=True).stdout
    if "@executable_path/../Frameworks" not in rpaths:
        run("install_name_tool", "-add_rpath", "@executable_path/../Frameworks", b)
for lib in copied.values():
    rpaths = subprocess.run(["otool", "-l", lib], capture_output=True, text=True).stdout
    if "@loader_path" not in rpaths:
        run("install_name_tool", "-add_rpath", "@loader_path", lib)

left = [d for f in binaries + list(copied.values()) for d, src in deps(f) if not d.startswith("@rpath/")]
if left:
    sys.exit(f"still linked to Homebrew: {left}")
size = sum(os.path.getsize(p) for p in copied.values())
print(f"bundled {len(copied)} libraries ({size / 1e6:.1f} MB)")
