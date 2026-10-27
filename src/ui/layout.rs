//! 布局基元。
//!
//! dashboard / drawer / toast 三处布局树共用的几何类型。
//! 之前每个模块各定义一份 `Rect2`，这里收敛为单一定义。

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect2 {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect2 {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    /// 闭区间命中：边界点算命中，避免像素级缝隙导致"看得见点不到"。
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px <= self.right() && py >= self.y && py <= self.bottom()
    }
    /// 两个矩形是否在水平/垂直方向重叠。
    ///
    /// 生产路径不需要它——它是给布局测试断言「元素不互相压盖」用的，
    /// 所以只在测试构建里编译。
    #[cfg(test)]
    pub fn overlaps(&self, other: &Rect2) -> bool {
        self.x < other.right() && other.x < self.right()
            && self.y < other.bottom() && other.y < self.bottom()
    }
}

/// 把布局矩形转成 GDI 的 RECT（right/bottom 为开区间边界）
pub fn to_rect(r: Rect2) -> windows_sys::Win32::Foundation::RECT {
    windows_sys::Win32::Foundation::RECT {
        left: r.x,
        top: r.y,
        right: r.right(),
        bottom: r.bottom(),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_is_inclusive_on_edges() {
        let r = Rect2::new(10, 10, 20, 20);
        assert!(r.contains(10, 10));
        assert!(r.contains(30, 30));
        assert!(!r.contains(31, 30));
        assert!(!r.contains(9, 10));
    }

    #[test]
    fn overlaps_detects_intersection() {
        let a = Rect2::new(0, 0, 10, 10);
        assert!(a.overlaps(&Rect2::new(5, 5, 10, 10)));
        assert!(!a.overlaps(&Rect2::new(10, 0, 10, 10)));
        assert!(!a.overlaps(&Rect2::new(0, 10, 10, 10)));
    }
}
