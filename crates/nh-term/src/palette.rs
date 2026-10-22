//! The default colours of a pane, before a program changes them.

use nh_proto::frame::Palette;

const BASE: [u32; 16] = [
    0x00_00_00_00,
    0x00_cd_00_00,
    0x00_00_cd_00,
    0x00_cd_cd_00,
    0x00_00_00_ee,
    0x00_cd_00_cd,
    0x00_00_cd_cd,
    0x00_e5_e5_e5,
    0x00_7f_7f_7f,
    0x00_ff_00_00,
    0x00_00_ff_00,
    0x00_ff_ff_00,
    0x00_5c_5c_ff,
    0x00_ff_00_ff,
    0x00_00_ff_ff,
    0x00_ff_ff_ff,
];

/// The `xterm` colours: 16 base colours, a 6×6×6 cube and 24 grays.
pub fn xterm_palette() -> Palette {
    let mut colors = [0u32; 256];
    colors[..16].copy_from_slice(&BASE);
    let level = |n: u32| if n == 0 { 0 } else { 55 + 40 * n };
    for i in 0..216u32 {
        let (r, g, b) = (level(i / 36), level(i / 6 % 6), level(i % 6));
        colors[16 + i as usize] = r << 16 | g << 8 | b;
    }
    for i in 0..24u32 {
        let gray = 8 + 10 * i;
        colors[232 + i as usize] = gray << 16 | gray << 8 | gray;
    }
    Palette {
        colors,
        fg: 0x00e5_e5e5,
        bg: 0x0000_0000,
        cursor: 0x00e5_e5e5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cube_and_the_gray_ramp_follow_xterm() {
        let p = xterm_palette();
        assert_eq!(p.colors[16], 0x000000);
        assert_eq!(p.colors[21], 0x0000ff);
        assert_eq!(p.colors[196], 0xff0000);
        assert_eq!(p.colors[231], 0xffffff);
        assert_eq!(p.colors[232], 0x080808);
        assert_eq!(p.colors[255], 0xeeeeee);
    }
}
