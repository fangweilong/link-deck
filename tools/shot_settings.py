"""设置页实拍：切五个分页各截一张。

用法（Windows）：python tools/shot_settings.py <输出目录>
要求 release 版 exe 已在运行。
"""
import ctypes
import ctypes.wintypes as wt
import os
import sys
import time

from PIL import ImageGrab

user32 = ctypes.windll.user32
user32.PostMessageW.argtypes = [wt.HWND, wt.UINT, wt.WPARAM, wt.LPARAM]
# HWND 是 64 位指针，默认 restype 会被截成 int32
user32.FindWindowW.restype = ctypes.c_void_p
user32.FindWindowW.argtypes = [wt.LPCWSTR, wt.LPCWSTR]
user32.GetWindowRect.argtypes = [wt.HWND, ctypes.POINTER(wt.RECT)]

WM_COMMAND = 0x0111
WM_LBUTTONDOWN = 0x0201
WM_LBUTTONUP = 0x0202
IDM_SHOW_SETTINGS = 1005

# 与 settings 里的常量保持一致：CARD_MARGIN=16、PILL_GAP=8、NAV_Y=TITLE_H+62
TITLE_H = 40
NAV_Y = TITLE_H + 62
NAV_H = 30
NAV_LABELS = [("界面", 76), ("悬浮球", 86), ("通知", 76), ("设备", 76), ("系统", 76)]


def find(cls):
    h = user32.FindWindowW(cls, None)
    if not h:
        raise SystemExit(f"找不到窗口类 {cls}")
    return h


def chip_centers():
    x = 16
    out = []
    for _label, w in NAV_LABELS:
        out.append(x + w // 2)
        x += w + 8
    return out


def click(hwnd, x, y):
    lp = (y << 16) | x
    user32.PostMessageW(hwnd, WM_LBUTTONDOWN, 1, lp)
    user32.PostMessageW(hwnd, WM_LBUTTONUP, 0, lp)


def shoot(hwnd, path):
    rect = wt.RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(rect))
    img = ImageGrab.grab(
        bbox=(rect.left, rect.top, rect.right, rect.bottom), all_screens=True
    )
    img.save(path)
    return img.size


def main():
    out_dir = sys.argv[1] if len(sys.argv) > 1 else "."
    os.makedirs(out_dir, exist_ok=True)

    host = find("LinkDeck_HiddenHost")
    user32.PostMessageW(host, WM_COMMAND, IDM_SHOW_SETTINGS, 0)
    time.sleep(0.8)

    panel = find("LinkDeck_ManagementPanel")
    user32.SetForegroundWindow(panel)
    time.sleep(0.3)

    for i, x in enumerate(chip_centers()):
        click(panel, x, NAV_Y + NAV_H // 2)
        time.sleep(0.45)
        name = NAV_LABELS[i][0]
        p = os.path.join(out_dir, f"page-{i}-{name}.png")
        print(name, shoot(panel, p), p)


if __name__ == "__main__":
    main()
