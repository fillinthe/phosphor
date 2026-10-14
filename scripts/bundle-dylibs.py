#!/usr/bin/env python3
"""Copies every Homebrew dylib a Mac app's binary needs into Contents/Frameworks and points the
binary and libraries at the copies, so the app runs on Macs without Homebrew.
Usage: bundle-dylibs.py path/to/App.app"""
import os, shutil, subprocess, sys

app = sys.argv[1]
exe_dir = os.path.join(app, "Contents", "MacOS")
fw = os.path.join(app, "Contents", "Frameworks")
os.makedirs(fw, exist_ok=True)

def deps(path):
    out = subprocess.run(["otool", "-L", path], capture_output=True, text=True, check=True).stdout
    for line in out.splitlines()[1:]:
        name = line.strip().split(" (")[0]
        if name.startswith(("/opt/homebrew/", "/usr/local/")):
            yield name

def run(*args):
    subprocess.run(args, check=True, capture_output=True)

binaries = [os.path.join(exe_dir, f) for f in os.listdir(exe_dir)]
todo = list(binaries)
copied = {}
while todo:
    f = todo.pop()
    for dep in deps(f):
        base = os.path.basename(dep)
        target = os.path.join(fw, base)
        if base not in copied:
            shutil.copy2(os.path.realpath(dep), target)
            os.chmod(target, 0o755)
            run("install_name_tool", "-id", f"@rpath/{base}", target)
            copied[base] = target
            todo.append(target)
        run("install_name_tool", "-change", dep, f"@rpath/{base}", f)

for b in binaries:
    rpaths = subprocess.run(["otool", "-l", b], capture_output=True, text=True).stdout
    if "@executable_path/../Frameworks" not in rpaths:
        run("install_name_tool", "-add_rpath", "@executable_path/../Frameworks", b)
for lib in copied.values():
    rpaths = subprocess.run(["otool", "-l", lib], capture_output=True, text=True).stdout
    if "@loader_path" not in rpaths:
        run("install_name_tool", "-add_rpath", "@loader_path", lib)

left = [d for f in binaries + list(copied.values()) for d in deps(f)]
if left:
    sys.exit(f"still linked to Homebrew: {left}")
size = sum(os.path.getsize(p) for p in copied.values())
print(f"bundled {len(copied)} libraries ({size / 1e6:.1f} MB)")
