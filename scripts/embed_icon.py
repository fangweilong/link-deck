import os
import struct
import ctypes
from ctypes import wintypes

kernel32 = ctypes.windll.kernel32

BeginUpdateResourceW = kernel32.BeginUpdateResourceW
BeginUpdateResourceW.argtypes = [wintypes.LPCWSTR, wintypes.BOOL]
BeginUpdateResourceW.restype = wintypes.HANDLE

UpdateResourceW = kernel32.UpdateResourceW
UpdateResourceW.argtypes = [wintypes.HANDLE, wintypes.LPCWSTR, wintypes.LPCWSTR, wintypes.WORD, wintypes.LPVOID, wintypes.DWORD]
UpdateResourceW.restype = wintypes.BOOL

EndUpdateResourceW = kernel32.EndUpdateResourceW
EndUpdateResourceW.argtypes = [wintypes.HANDLE, wintypes.BOOL]
EndUpdateResourceW.restype = wintypes.BOOL

RT_ICON = ctypes.cast(3, wintypes.LPCWSTR)
RT_GROUP_ICON = ctypes.cast(14, wintypes.LPCWSTR)

def embed_ico_to_exe(exe_path, ico_path):
    if not os.path.exists(exe_path):
        print(f"File not found: {exe_path}")
        return
    with open(ico_path, "rb") as f:
        ico_data = f.read()

    reserved, ico_type, count = struct.unpack_from("<HHH", ico_data, 0)
    entries = []
    offset = 6
    for i in range(count):
        w, h, colors, res, planes, bpp, size, img_offset = struct.unpack_from("<BBBBHHII", ico_data, offset)
        offset += 16
        img_data = ico_data[img_offset:img_offset + size]
        entries.append((w, h, colors, res, planes, bpp, size, img_data))

    h_update = BeginUpdateResourceW(exe_path, False)
    if not h_update:
        err = ctypes.GetLastError()
        raise RuntimeError(f"BeginUpdateResourceW failed on {exe_path}: {err}")

    for idx, entry in enumerate(entries):
        icon_id = idx + 1
        img_data = entry[7]
        res_id = ctypes.cast(icon_id, wintypes.LPCWSTR)
        ok = UpdateResourceW(h_update, RT_ICON, res_id, 0x0409, img_data, len(img_data))
        if not ok:
            err = ctypes.GetLastError()
            raise RuntimeError(f"UpdateResourceW for RT_ICON {icon_id} failed: {err}")

    grp_dir = struct.pack("<HHH", 0, 1, count)
    grp_entries = b""
    for idx, entry in enumerate(entries):
        icon_id = idx + 1
        w, h, colors, res, planes, bpp, size, _ = entry
        grp_entries += struct.pack("<BBBBHHIH", w, h, colors, res, planes, bpp, size, icon_id)

    grp_data = grp_dir + grp_entries
    group_res_id = ctypes.cast(1, wintypes.LPCWSTR)
    ok = UpdateResourceW(h_update, RT_GROUP_ICON, group_res_id, 0x0409, grp_data, len(grp_data))
    if not ok:
        err = ctypes.GetLastError()
        raise RuntimeError(f"UpdateResourceW for RT_GROUP_ICON failed: {err}")

    ok = EndUpdateResourceW(h_update, False)
    if not ok:
        err = ctypes.GetLastError()
        raise RuntimeError(f"EndUpdateResourceW failed: {err}")

    print(f"Successfully embedded {count} icon resolutions into {exe_path}!")

if __name__ == "__main__":
    ico_path = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "linkdeck.ico")
    for p in ["target/release/link-deck.exe", "target/debug/link-deck.exe"]:
        embed_ico_to_exe(p, ico_path)
