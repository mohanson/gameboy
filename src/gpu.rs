use crate::convention::{Global, Memory, OamBug, SCREEN_H, SCREEN_W, Signal, Term, Ticker};
use crate::interrupt::{Interrupt, InterruptFlag};
use std::cell::RefCell;
use std::rc::Rc;

// LCDC is the main LCD Control register. Its bits toggle what elements are displayed on the screen, and how.
pub struct Lcdc {
    data: u8,
}

#[rustfmt::skip]
impl Lcdc {
    pub fn power_up() -> Self {
        Self { data: 0x48 }
    }

    // LCDC.7 - LCD Display Enable
    // This bit controls whether the LCD is on and the PPU is active. Setting it to 0 turns both off, which grants
    // immediate and full access to VRAM, OAM, etc.
    pub fn bit7(&self) -> bool { self.data & 0b1000_0000 != 0x00 }

    // LCDC.6 - Window Tile Map Display Select
    // This bit controls which background map the Window uses for rendering. When it's reset, the $9800 tilemap is used,
    // otherwise it's the $9C00 one.
    pub fn bit6(&self) -> bool { self.data & 0b0100_0000 != 0x00 }

    // LCDC.5 - Window Display Enable
    // This bit controls whether the window shall be displayed or not. (TODO : what happens when toggling this
    // mid-scanline ?) This bit is overridden on DMG by bit 0 if that bit is reset.
    // Note that on CGB models, setting this bit to 0 then back to 1 mid-frame may cause the second write to be ignored.
    pub fn bit5(&self) -> bool { self.data & 0b0010_0000 != 0x00 }

    // LCDC.4 - BG & Window Tile Data Select
    // This bit controls which addressing mode the BG and Window use to pick tiles.
    // Sprites aren't affected by this, and will always use $8000 addressing mode.
    pub fn bit4(&self) -> bool { self.data & 0b0001_0000 != 0x00 }

    // LCDC.3 - BG Tile Map Display Select
    // This bit works similarly to bit 6: if the bit is reset, the BG uses tilemap $9800, otherwise tilemap $9C00.
    pub fn bit3(&self) -> bool { self.data & 0b0000_1000 != 0x00 }

    // LCDC.2 - OBJ Size
    // This bit controls the sprite size (1 tile or 2 stacked vertically).
    // Be cautious when changing this mid-frame from 8x8 to 8x16 : "remnants" of the sprites intended for 8x8 could
    // "leak" into the 8x16 zone and cause artifacts.
    pub fn bit2(&self) -> bool { self.data & 0b0000_0100 != 0x00 }

    // LCDC.1 - OBJ Display Enable
    // This bit toggles whether sprites are displayed or not.
    // This can be toggled mid-frame, for example to avoid sprites being displayed on top of a status bar or text box.
    // (Note: toggling mid-scanline might have funky results on DMG? Investigation needed.)
    pub fn bit1(&self) -> bool { self.data & 0b0000_0010 != 0x00 }


    // LCDC.0 - BG/Window Display/Priority
    // LCDC.0 has different meanings depending on Gameboy type and Mode:
    // Monochrome Gameboy, SGB and CGB in Non-CGB Mode: BG Display
    // When Bit 0 is cleared, both background and window become blank (white), and the Window Display Bit is ignored in
    // that case. Only Sprites may still be displayed (if enabled in Bit 1).
    // CGB in CGB Mode: BG and Window Master Priority
    // When Bit 0 is cleared, the background and window lose their priority - the sprites will be always displayed on
    // top of background and window, independently of the priority flags in OAM and BG Map attributes.
    pub fn bit0(&self) -> bool { self.data & 0b0000_0001 != 0x00 }
}

// LCD Status Register.
pub struct Stat {
    data: u8,
}

#[rustfmt::skip]
impl Stat {
    pub fn power_up() -> Self {
        Self { data: 0x00 }
    }

    // Stat.6 - LYC=LY Coincidence Interrupt (1=Enable) (Read/Write)
    pub fn bit6(&self) -> bool { self.data & 0b0100_0000 != 0x00 }

    // Stat.5 - Mode 2 OAM Interrupt         (1=Enable) (Read/Write)
    pub fn bit5(&self) -> bool { self.data & 0b0010_0000 != 0x00 }

    // Stat.4 - Mode 1 V-Blank Interrupt     (1=Enable) (Read/Write)
    pub fn bit4(&self) -> bool { self.data & 0b0001_0000 != 0x00 }

    // Stat.3 - Mode 0 H-Blank Interrupt     (1=Enable) (Read/Write)
    pub fn bit3(&self) -> bool { self.data & 0b0000_1000 != 0x00 }

    // Stat.2 - LYC=LY Coincidence Flag (0=Different, 1=Equal) (Read Only)
    pub fn bit2(&self) -> bool { self.data & 0b0000_0100 != 0x00 }

    // Set or clear the LYC=LY coincidence flag (Stat.2).
    pub fn lyeq(&mut self, v: bool) { if v { self.data |= 0b0000_0100 } else { self.data &= !0b0000_0100 } }

    // Stat.0 - PPU Mode (0=HBlank, 1=VBlank, 2=OAM Scan, 3=Pixel Transfer) (Read Only)
    pub fn mode(&self) -> u8 { self.data & 0x03 }
}

// This register is used to address a byte in the CGBs Background Palette Memory. Each two byte in that memory define a
// color value. The first 8 bytes define Color 0-3 of Palette 0 (BGP0), and so on for BGP1-7.
//  Bit 0-5   Index (00-3F)
//  Bit 7     Auto Increment  (0=Disabled, 1=Increment after Writing)
// Data can be read/written to/from the specified index address through Register FF69. When the Auto Increment bit is
// set then the index is automatically incremented after each <write> to FF69. Auto Increment has no effect when
// <reading> from FF69, so the index must be manually incremented in that case. Writing to FF69 during rendering still
// causes auto-increment to occur.
// Unlike the following, this register can be accessed outside V-Blank and H-Blank.
pub struct Bgpi {
    data: u8,
}

impl Bgpi {
    pub fn power_up() -> Self {
        Self { data: 0x00 }
    }

    // Bgpi.5-0 - Palette Memory Address (00-3F)
    pub fn addr(&self) -> u8 {
        self.data & 0x3f
    }

    // Bgpi.7 - Auto Increment (0=Disabled, 1=Increment after Writing)
    pub fn auto(&self) -> bool {
        self.data & 0b1000_0000 != 0x00
    }

    // When the auto increment bit is set then the index is automatically incremented after each write to FF69.
    pub fn incr(&mut self) {
        self.data = (self.data & 0x80) | self.addr().wrapping_add(1) & 0x3f;
    }
}

impl Memory for Bgpi {
    fn lb(&self, _: u16) -> u8 {
        self.data
    }

    fn sb(&mut self, _: u16, v: u8) {
        self.data = v & 0xbf;
    }
}

// See: https://gbdev.io/pandocs/OAM.html#byte-3--attributesflags
struct Attr {
    data: u8,
}

#[rustfmt::skip]
impl Attr {
    // Attr.7 - OBJ-to-BG Priority (0=OBJ Above BG, 1=OBJ Behind BG color 1-3)
    fn bit7(&self) -> bool { self.data & 0b1000_0000 != 0x00 }

    // Attr.6 - Y Flip (0=Normal, 1=Vertically mirrored)
    fn bit6(&self) -> bool { self.data & 0b0100_0000 != 0x00 }

    // Attr.5 - X Flip (0=Normal, 1=Horizontally mirrored)
    fn bit5(&self) -> bool { self.data & 0b0010_0000 != 0x00 }

    // Attr.4 - DMG Palette [Non-CGB only] (0=OBP0, 1=OBP1)
    fn bit4(&self) -> bool { self.data & 0b0001_0000 != 0x00 }

    // Attr.3 - Tile VRAM Bank [CGB only] (0=Bank 0, 1=Bank 1)
    fn bit3(&self) -> bool { self.data & 0b0000_1000 != 0x00 }

    // Attr.2-0 - CGB Palette Number [CGB only] (OBP0-7)
    fn paln(&self) -> usize { (self.data & 0x07) as usize }
}

impl From<u8> for Attr {
    fn from(u: u8) -> Self {
        Self { data: u }
    }
}

pub struct Gpu {
    glo: Rc<RefCell<Global>>,

    bcps: Bgpi,
    // This register allows to read/write data to the CGBs Background Palette Memory, addressed through Register FF68.
    // Each color is defined by two bytes (Bit 0-7 in first byte).
    //     Bit 0-4   Red Intensity   (00-1F)
    //     Bit 5-9   Green Intensity (00-1F)
    //     Bit 10-14 Blue Intensity  (00-1F)
    // Much like VRAM, data in Palette Memory cannot be read/written during the time when the LCD Controller is
    // reading from it. (That is when the STAT register indicates Mode 3). Note: All background colors are initialized
    // as white by the boot ROM, but it's a good idea to initialize at least one color yourself (for example if you
    // include a soft-reset mechanic).
    //
    // Note: Type [[[u8; 3]; 4]; 8] equals with [u8; 64].
    bcpd: [[[u8; 3]; 4]; 8],
    // Digital image with mode RGB. Size = 144 * 160 * 3.
    // 3---------
    // ----------
    // ----------
    // ---------- 160
    //        144
    data: [[[u8; 3]; SCREEN_W]; SCREEN_H],
    // The LCD controller operates on a 222 Hz = 4.194 MHz dot clock. An entire frame is 154 scanlines, 70224 dots, or
    // 16.74 ms. On scanlines 0 through 143, the LCD controller cycles through modes 2, 3, and 0 once every 456 dots.
    // Scanlines 144 through 153 are mode 1.
    dots: u32,
    lcdc: Lcdc,
    ocps: Bgpi,
    ocpd: [[[u8; 3]; 4]; 8],
    // Extra dots to add to the mode3 threshold due to sprites on the current scanline. Computed when mode2 fires;
    // represents floor(T_penalty/4)*4 extra dots before mode0.
    pena: u32,
    // BG priority per pixel: bit 7 = bg_prio (attr.bit7), bits 0-1 = color index.
    prio: [u8; SCREEN_W],
    sigh: Signal,
    // Tracks the STAT IRQ signal level (the OR of all enabled STAT interrupt sources). The LCD interrupt (IF bit 1) is
    // only raised on a 0->1 RISING EDGE of this signal. This implements the "STAT IRQ blocking" behaviour: if one
    // source (e.g. LYC=LY) keeps the signal HIGH through mode 3, the mode-0 transition does NOT generate a second
    // interrupt because the signal never goes LOW in between.
    sigq: u8,
    // When a new scene is rendered, synchronization is triggered.
    sigv: Signal,
    stat: Stat,
    ram: [u8; 0x4000],
    rbk: usize,
    // VRAM Sprite Attribute Table (OAM)
    // Gameboy video controller can display up to 40 sprites either in 8x8 or in 8x16 pixels. Because of a limitation of
    // hardware, only ten sprites can be displayed per scan line. Sprite patterns have the same format as BG tiles, but
    // they are taken from the Sprite Pattern Table located at $8000-8FFF and have unsigned numbering.
    // Sprite attributes reside in the Sprite Attribute Table (OAM - Object Attribute Memory) at $FE00-FE9F. Each of the 40
    // entries consists of four bytes with the following meanings:
    // Byte0 - Y Position
    // Specifies the sprites vertical position on the screen (minus 16). An off-screen value (for example, Y=0 or
    // Y>=160) hides the sprite.
    //
    // Byte1 - X Position
    // Specifies the sprites horizontal position on the screen (minus 8). An off-screen value (X=0 or X>=168) hides the
    // sprite, but the sprite still affects the priority ordering - a better way to hide a sprite is to set its
    // Y-coordinate off-screen.
    //
    // Byte2 - Tile/Pattern Number
    // Specifies the sprites Tile Number (00-FF). This (unsigned) value selects a tile from memory at 8000h-8FFFh. In
    // CGB Mode this could be either in VRAM Bank 0 or 1, depending on Bit 3 of the following byte. In 8x16 mode, the
    // lower bit of the tile number is ignored. IE: the upper 8x8 tile is "NN AND FEh", and the lower 8x8 tile
    // is "NN OR 01h".
    //
    // Byte3 - Attributes/Flags:
    // Bit7   OBJ-to-BG Priority (0=OBJ Above BG, 1=OBJ Behind BG color 1-3)
    //        (Used for both BG and Window. BG color 0 is always behind OBJ)
    // Bit6   Y flip          (0=Normal, 1=Vertically mirrored)
    // Bit5   X flip          (0=Normal, 1=Horizontally mirrored)
    // Bit4   Palette number  **Non CGB Mode Only** (0=OBP0, 1=OBP1)
    // Bit3   Tile VRAM-Bank  **CGB Mode Only**     (0=Bank 0, 1=Bank 1)
    // Bit2-0 Palette number  **CGB Mode Only**     (OBP0-7)
    oam: [u8; 0xa0],
    // This register assigns gray shades to the color numbers of the BG and Window tiles.
    bgp: u8,
    // This register assigns gray shades for sprite palette 0. It works exactly as BGP (FF47), except that the lower
    // two bits aren't used because sprite data 00 is transparent.
    op0: u8,
    // This register assigns gray shades for sprite palette 1. It works exactly as BGP (FF47), except that the lower
    // two bits aren't used because sprite data 00 is transparent.
    op1: u8,
    // Scroll Y (R/W), Scroll X (R/W).
    // Specifies the position in the 256x256 pixels BG map (32x32 tiles) which is to be displayed at the upper/left LCD
    // display position. Values in range from 0-255 may be used for X/Y each, the video controller automatically wraps
    // back to the upper (left) position in BG map when drawing exceeds the lower (right) border of the BG map area.
    sx: u8,
    sy: u8,
    // Window Y Position (R/W), Window X Position minus 7 (R/W).
    wx: u8,
    wy: u8,
    // The Gameboy permanently compares the value of the LYC and LY registers. When both values are identical, the
    // coincident bit in the STAT register becomes set, and (if enabled) a STAT interrupt is requested.
    lc: u8,
    // The LY indicates the vertical line to which the present data is transferred to the LCD Driver. The LY can take
    // on any value between 0 through 153. The values between 144 and 153 indicate the V-Blank period. Writing will
    // reset the counter.
    ly: u8,
}

impl Gpu {
    pub fn power_up(glo: Rc<RefCell<Global>>) -> Self {
        Self {
            glo,
            bcps: Bgpi::power_up(),
            bcpd: [[[0u8; 3]; 4]; 8],
            data: [[[0xffu8; 3]; SCREEN_W]; SCREEN_H],
            dots: 0,
            lcdc: Lcdc::power_up(),
            ocps: Bgpi::power_up(),
            ocpd: [[[0u8; 3]; 4]; 8],
            pena: 0,
            prio: [0x80; SCREEN_W],
            sigh: Signal::power_up(),
            sigq: 0x00,
            sigv: Signal::power_up(),
            stat: Stat::power_up(),
            ram: [0x00; 0x4000],
            rbk: 0x00,
            oam: [0x00; 0xa0],
            bgp: 0x00,
            op0: 0x00,
            op1: 0x01,
            sx: 0x00,
            sy: 0x00,
            wx: 0x00,
            wy: 0x00,
            lc: 0x00,
            ly: 0x00,
        }
    }

    pub fn image(&self) -> &[[[u8; 3]; SCREEN_W]; SCREEN_H] {
        &self.data
    }

    // DMG OAM corruption (mode 2 only). Row 0 and rows ≥20 are immune.
    pub fn oam_corrupt(&mut self, kind: OamBug) {
        if !self.lcdc.bit7() || self.stat.mode() != 2 || self.ly >= 144 {
            return;
        }
        let row = (self.dots / 4) as usize;
        if row == 0 || row >= 20 {
            return;
        }
        let cur = row * 8;
        let old = cur - 8;
        if kind == OamBug::Rdi && row >= 4 && row < 19 {
            let ppp = (row - 2) * 8;
            let a = self.oam_lh(ppp);
            let b = self.oam_lh(old);
            let c = self.oam_lh(cur);
            let d = self.oam_lh(old + 4);
            self.oam_sh(old, (b & (a | c | d)) | (a & c & d));
            self.oam.copy_within(old..old + 8, ppp);
            self.oam.copy_within(old..old + 8, cur);
        }
        let a = self.oam_lh(cur);
        let b = self.oam_lh(old);
        let c = self.oam_lh(old + 4);
        self.oam_sh(
            cur,
            match kind {
                OamBug::Idu => ((a ^ c) & (b ^ c)) ^ c,
                OamBug::Seq => b | (a & c),
                OamBug::Rdi => b | (a & c),
            },
        );
        self.oam.copy_within(old + 2..old + 8, cur + 2);
    }

    pub fn sigh_censor(&mut self) -> bool {
        self.sigh.get()
    }

    // Update the STAT IRQ wired-OR signal and fire an LCD interrupt on a 0->1 rising edge. LCD off is a no-op:
    // sigq stays frozen so that re-enabling the LCD cannot produce a spurious edge from the implied LOW-while-off
    // state.
    pub fn sigq_update(&mut self) {
        if !self.lcdc.bit7() {
            return;
        }
        let ca = self.stat.bit3() && self.stat.mode() == 0;
        let cb = self.stat.bit4() && self.stat.mode() == 1;
        let cc = self.stat.bit5() && self.stat.mode() == 2;
        let cd = self.stat.bit6() && self.stat.bit2();
        let sigq_new = ca || cb || cc || cd;
        let sigq_new = sigq_new as u8;
        if self.sigq == 0x00 && sigq_new == 0x01 {
            Interrupt::owned(self.glo.clone()).raise(InterruptFlag::LCD);
        }
        self.sigq = sigq_new;
    }

    pub fn sigv_censor(&mut self) -> bool {
        self.sigv.get()
    }

    fn draw_bg(&mut self) {
        let base = if self.lcdc.bit4() { 0x8000u16 } else { 0x8800u16 };
        let show = self.lcdc.bit5() && self.wy <= self.ly;
        let wx = self.wx.wrapping_sub(7);
        let py = if show { self.ly.wrapping_sub(self.wy) } else { self.sy.wrapping_add(self.ly) };
        let ty = (u16::from(py) >> 3) & 31;

        for x in 0..SCREEN_W {
            let within = show && x as u8 >= wx;
            let px = if within { x as u8 - wx } else { self.sx.wrapping_add(x as u8) };
            let tx = (u16::from(px) >> 3) & 31;
            // Tilemap base: bit6 selects window map, bit3 selects BG map.
            let tb: u16 = if within {
                if self.lcdc.bit6() { 0x9c00 } else { 0x9800 }
            } else {
                if self.lcdc.bit3() { 0x9c00 } else { 0x9800 }
            };
            let addr = tb + ty * 32 + tx;
            let tn = self.ram0_lb(addr);
            // bit4=1: unsigned $8000 addressing; bit4=0: signed $8800 addressing.
            let toff = if self.lcdc.bit4() { i16::from(tn) } else { i16::from(tn as i8) + 128 } as u16 * 16;
            let tloc = base + toff;
            let attr = Attr::from(self.ram1_lb(addr));
            let fy = if attr.bit6() { 7 - py % 8 } else { py % 8 };
            // CGB attr.bit3: read tile data from VRAM bank 1 instead of bank 0.
            let td: [u8; 2] = if self.glo.borrow().term == Term::CGB && attr.bit3() {
                [self.ram1_lb(tloc + u16::from(fy * 2)), self.ram1_lb(tloc + u16::from(fy * 2) + 1)]
            } else {
                [self.ram0_lb(tloc + u16::from(fy * 2)), self.ram0_lb(tloc + u16::from(fy * 2) + 1)]
            };
            let fx = if attr.bit5() { 7 - px % 8 } else { px % 8 };
            // Decode 2bpp: one bit from each plane gives a 2-bit color index.
            let bit = 0x80 >> fx;
            let col = ((td[1] & bit != 0) as usize) << 1 | (td[0] & bit != 0) as usize;
            self.prio[x] = (attr.bit7() as u8) << 7 | col as u8;
            match self.glo.clone().borrow().term {
                Term::DMG => self.pixel_gre(x, self.gray_shades(self.bgp, col)),
                Term::CGB => {
                    let [r, g, b] = self.bcpd[attr.paln()][col];
                    self.pixel_rgb(x, r, g, b)
                }
            }
        }
    }

    // Gameboy video controller can display up to 40 sprites either in 8x8 or in 8x16 pixels. Because of a limitation
    // of hardware, only ten sprites can be displayed per scan line. Sprite patterns have the same format as BG tiles,
    // but they are taken from the Sprite Pattern Table located at $8000-8FFF and have unsigned numbering.
    //
    // Sprite attributes reside in the Sprite Attribute Table (OAM - Object Attribute Memory) at $FE00-FE9F. Each of
    // the 40 entries consists of four bytes with the following meanings:
    //   Byte0 - Y Position
    //   Specifies the sprites vertical position on the screen (minus 16). An off-screen value (for example, Y=0 or
    //   Y>=160) hides the sprite.
    //
    //   Byte1 - X Position
    //   Specifies the sprites horizontal position on the screen (minus 8). An off-screen value (X=0 or X>=168) hides
    //   the sprite, but the sprite still affects the priority ordering - a better way to hide a sprite is to set its
    //   Y-coordinate off-screen.
    //
    //   Byte2 - Tile/Pattern Number
    //   Specifies the sprites Tile Number (00-FF). This (unsigned) value selects a tile from memory at 8000h-8FFFh. In
    //   CGB Mode this could be either in VRAM Bank 0 or 1, depending on Bit 3 of the following byte. In 8x16 mode, the
    //   lower bit of the tile number is ignored. IE: the upper 8x8 tile is "NN AND FEh", and the lower 8x8 tile is
    //   "NN OR 01h".
    //
    //   Byte3 - Attributes/Flags:
    //     Bit7   OBJ-to-BG Priority (0=OBJ Above BG, 1=OBJ Behind BG color 1-3)
    //           (Used for both BG and Window. BG color 0 is always behind OBJ)
    //     Bit6   Y flip          (0=Normal, 1=Vertically mirrored)
    //     Bit5   X flip          (0=Normal, 1=Horizontally mirrored)
    //     Bit4   Palette number  **Non CGB Mode Only** (0=OBP0, 1=OBP1)
    //     Bit3   Tile VRAM-Bank  **CGB Mode Only**     (0=Bank 0, 1=Bank 1)
    //     Bit2-0 Palette number  **CGB Mode Only**     (OBP0-7)
    fn draw_sprites(&mut self) {
        let sz = if self.lcdc.bit2() { 16u8 } else { 8u8 }; // sprite height: 8 or 16
        for i in 0..40 {
            let oa = 0xfe00 + (i as u16) * 4; // OAM entry base address
            let py = self.lb(oa).wrapping_sub(16);
            let px = self.lb(oa + 1).wrapping_sub(8);
            let tn = self.lb(oa + 2) & if self.lcdc.bit2() { 0xfe } else { 0xff };
            let at = Attr::from(self.lb(oa + 3));
            // Skip if current scanline is outside the sprite's vertical span.
            if self.ly.wrapping_sub(py) >= sz {
                continue;
            }
            // Skip sprites fully off-screen horizontally (px in [SCREEN_W, 0xf8]).
            if px >= SCREEN_W as u8 && px <= 0xf8 {
                continue;
            }
            // Tile row within the sprite, accounting for Y-flip.
            let ty = if at.bit6() { sz - 1 - self.ly.wrapping_sub(py) } else { self.ly.wrapping_sub(py) };
            let ta = 0x8000u16 + u16::from(tn) * 16 + u16::from(ty) * 2;
            // Fetch 2bpp tile row; CGB sprites may use VRAM bank 1.
            let td = if self.glo.borrow().term == Term::CGB && at.bit3() {
                [self.ram1_lb(ta), self.ram1_lb(ta + 1)]
            } else {
                [self.ram0_lb(ta), self.ram0_lb(ta + 1)]
            };
            for x in 0u8..8 {
                let sx = px.wrapping_add(x);
                if sx >= SCREEN_W as u8 {
                    continue;
                }
                let tx = if at.bit5() { 7 - x } else { x };
                // Decode 2bpp color index (0 = transparent).
                let bit = 0x80u8 >> tx;
                let col = ((td[1] & bit != 0) as usize) << 1 | (td[0] & bit != 0) as usize;
                if col == 0 {
                    continue;
                }
                // BG/sprite priority arbitration.
                let pr = self.prio[sx as usize];
                let pr_col = (pr & 0x03) as usize;
                let pr_prio = pr & 0x80 != 0;
                let skip = if self.glo.borrow().term == Term::CGB && !self.lcdc.bit0() {
                    pr_col == 0
                } else if pr_prio {
                    pr_col != 0
                } else {
                    at.bit7() && pr_col != 0
                };
                if skip {
                    continue;
                }
                match self.glo.clone().borrow().term {
                    Term::DMG => {
                        let pal = if at.bit4() { self.op1 } else { self.op0 };
                        self.pixel_gre(sx as usize, self.gray_shades(pal, col));
                    }
                    Term::CGB => {
                        let [r, g, b] = self.ocpd[at.paln()][col];
                        self.pixel_rgb(sx as usize, r, g, b);
                    }
                }
            }
        }
    }

    // This register assigns gray shades to the color numbers of the BG and Window tiles.
    // Bit 7-6 - Shade for Color Number 3
    // Bit 5-4 - Shade for Color Number 2
    // Bit 3-2 - Shade for Color Number 1
    // Bit 1-0 - Shade for Color Number 0
    // The four possible gray shades are:
    // 0  White 0xff
    // 1  Light 0xc0
    // 2  Dusky 0x60
    // 3  Black 0x00
    fn gray_shades(&self, v: u8, i: usize) -> u8 {
        (v >> (2 * i)) & 0x03
    }

    fn oam_lh(&self, off: usize) -> u16 {
        u16::from(self.oam[off]) | (u16::from(self.oam[off + 1]) << 8)
    }

    fn oam_sh(&mut self, off: usize, v: u16) {
        self.oam[off] = v as u8;
        self.oam[off + 1] = (v >> 8) as u8;
    }

    // Compute the sprite timing penalty (in T-cycles) for the current scanline. Each sprite on the scanline with
    // OAM_X < 168 adds:
    // 0. 6T (tile fetch cost)
    // 1. max(0, 5 - (oam_x + scx) % 8) T (alignment cost, once per unique X)
    // Only the first 10 sprites per scanline are counted (DMG hardware limit).
    fn pena_calc(&self) -> u32 {
        if !self.lcdc.bit1() {
            return 0;
        }
        let sz: i32 = if self.lcdc.bit2() { 16 } else { 8 };
        let ly = self.ly as i32;
        let mut pena = 0u32;
        let mut xs = [0x00; 256];
        let mut n = 0u32;
        for i in 0..40 {
            let oy = self.oam[i * 4] as i32;
            let ox = self.oam[i * 4 + 1];
            if ly < oy - 16 || ly > oy - 17 + sz || ox >= 168 {
                continue;
            }
            n += 1;
            if n > 10 {
                break;
            }
            pena += 6;
            if xs[ox as usize] == 0x00 {
                xs[ox as usize] = 0x01;
                let fine = (ox as u32 + self.sx as u32) % 8;
                if fine < 5 {
                    pena += 5 - fine;
                }
            }
        }
        pena
    }

    // Grey scale.
    fn pixel_gre(&mut self, x: usize, g: u8) {
        self.data[self.ly as usize][x] = [g, g, g];
    }

    // When developing graphics on PCs, note that the RGB values will have different appearance on CGB displays as on
    // VGA/HDMI monitors calibrated to sRGB color. Because the GBC is not lit, the highest intensity will produce Light
    // Gray color rather than White. The intensities are not linear; the values 10h-1Fh will all appear very bright,
    // while medium and darker colors are ranged at 00h-0Fh.
    // The CGB display's pigments aren't perfectly saturated. This means the colors mix quite oddly; increasing
    // intensity of only one R,G,B color will also influence the other two R,G,B colors. For example, a color setting
    // of 03EFh (Blue=0, Green=1Fh, Red=0Fh) will appear as Neon Green on VGA displays, but on the CGB it'll produce a
    // decently washed out Yellow. See image on the right.
    fn pixel_rgb(&mut self, x: usize, r: u8, g: u8, b: u8) {
        assert!(r <= 0x1f);
        assert!(g <= 0x1f);
        assert!(b <= 0x1f);
        let r = u32::from(r);
        let g = u32::from(g);
        let b = u32::from(b);
        let lr = ((r * 13 + g * 2 + b) >> 1) as u8;
        let lg = ((g * 3 + b) << 1) as u8;
        let lb = ((r * 3 + g * 2 + b * 11) >> 1) as u8;
        self.data[self.ly as usize][x] = [lr, lg, lb];
    }

    fn ram0_lb(&self, a: u16) -> u8 {
        self.ram[a as usize - 0x8000]
    }

    fn ram1_lb(&self, a: u16) -> u8 {
        self.ram[a as usize - 0x6000]
    }
}

impl Memory for Gpu {
    fn lb(&self, a: u16) -> u8 {
        match a {
            0x8000..=0x9fff => {
                // VRAM is locked during mode 3, and also during the 4T pre-mode-3 period (mode 2, dots >= 76) where
                // the bus is already claimed by the GPU, matching real DMG hardware bus timing.
                let locked = self.stat.mode() == 3 || (self.stat.mode() == 2 && self.dots >= 76);
                if !locked { self.ram[self.rbk * 0x2000 + a as usize - 0x8000] } else { 0xff }
            }
            0xfe00..=0xfe9f => {
                // OAM is locked (returns 0xFF) during mode 2 (OAM scan) and mode 3 (pixel transfer). It is also locked
                // from dot 452 onward (LY increment / OAM scan preparation), even though STAT still reports mode 0 at
                // that dot.
                let locked =
                    self.stat.mode() == 2 || self.stat.mode() == 3 || (self.stat.mode() == 0 && self.dots >= 452);
                if !locked { self.oam[a as usize - 0xfe00] } else { 0xff }
            }
            0xff40 => self.lcdc.data,
            0xff41 => self.stat.data | 0x80,
            0xff42 => self.sy,
            0xff43 => self.sx,
            0xff44 => self.ly,
            0xff45 => self.lc,
            0xff47 => self.bgp,
            0xff48 => self.op0,
            0xff49 => self.op1,
            0xff4a => self.wy,
            0xff4b => self.wx,
            0xff4f => 0xfe | self.rbk as u8,
            0xff68 => self.bcps.lb(0xff68),
            0xff69 => {
                let r = self.bcps.addr() as usize >> 3;
                let c = self.bcps.addr() as usize >> 1 & 0x3;
                if self.bcps.addr() & 0x01 == 0x00 {
                    let a = self.bcpd[r][c][0];
                    let b = self.bcpd[r][c][1] << 5;
                    a | b
                } else {
                    let a = self.bcpd[r][c][1] >> 3;
                    let b = self.bcpd[r][c][2] << 2;
                    a | b
                }
            }
            0xff6a => self.ocps.lb(0xff6a),
            0xff6b => {
                let r = self.ocps.addr() as usize >> 3;
                let c = self.ocps.addr() as usize >> 1 & 0x3;
                if self.ocps.addr() & 0x01 == 0x00 {
                    let a = self.ocpd[r][c][0];
                    let b = self.ocpd[r][c][1] << 5;
                    a | b
                } else {
                    let a = self.ocpd[r][c][1] >> 3;
                    let b = self.ocpd[r][c][2] << 2;
                    a | b
                }
            }
            _ => unreachable!(),
        }
    }

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0x8000..=0x9fff => {
                if self.stat.mode() == 3 {
                    return;
                }
                self.ram[self.rbk * 0x2000 + a as usize - 0x8000] = v;
            }
            0xfe00..=0xfe9f => {
                // OAM writes are ignored during mode 3 (pixel transfer) and during mode 2 (OAM scan) while dots < 76.
                // The last 4T of mode 2 (dots 76-79) the OAM scan is already complete and the CPU can write to OAM
                // again.
                let locked = self.stat.mode() == 3 || (self.stat.mode() == 2 && self.dots < 76);
                if !locked {
                    self.oam[a as usize - 0xfe00] = v;
                }
            }
            0xff40 => {
                let old = self.lcdc.bit7() as u8;
                self.lcdc.data = v;
                let now = self.lcdc.bit7() as u8;
                match (old << 1) | now {
                    0x00 | 0x02 => {
                        self.dots = 0;
                        self.ly = 0;
                        self.stat.data &= !0x03;
                        self.data = [[[0xffu8; 3]; SCREEN_W]; SCREEN_H];
                        self.sigv.set();
                        self.sigq_update();
                    }
                    0x01 => {
                        self.stat.lyeq(self.ly == self.lc);
                        self.sigq_update();
                    }
                    0x03 => {}
                    _ => unreachable!(),
                }
            }
            0xff41 => {
                // Bits 3-6 are writable; bits 0-1 (mode) are read-only (PPU-controlled).
                self.stat.data = 0x80 | (v & 0x78) | (self.stat.data & 0x07);
                // Enabling a source that is currently active generates a rising edge.
                self.sigq_update();
            }
            0xff42 => self.sy = v,
            0xff43 => self.sx = v,
            0xff44 => {}
            0xff45 => {
                self.lc = v;
                // The comparison clock only runs while the LCD is on. Writing LYC while LCD is off must not change
                // stat.bit2() (STAT bit 2 is frozen during LCD-off).
                if !self.lcdc.bit7() {
                    return;
                }
                self.stat.lyeq(self.ly == self.lc);
                self.sigq_update();
            }
            0xff47 => self.bgp = v,
            0xff48 => self.op0 = v,
            0xff49 => self.op1 = v,
            0xff4a => self.wy = v,
            0xff4b => self.wx = v,
            0xff4f => self.rbk = (v & 0x01) as usize,
            0xff68 => self.bcps.sb(0xff68, v),
            0xff69 => {
                let r = self.bcps.addr() as usize >> 3;
                let c = self.bcps.addr() as usize >> 1 & 0x03;
                if self.bcps.addr() & 0x01 == 0x00 {
                    self.bcpd[r][c][0] = v & 0x1f;
                    self.bcpd[r][c][1] = (self.bcpd[r][c][1] & 0x18) | (v >> 5);
                } else {
                    self.bcpd[r][c][1] = (self.bcpd[r][c][1] & 0x07) | ((v & 0x03) << 3);
                    self.bcpd[r][c][2] = (v >> 2) & 0x1f;
                }
                if self.bcps.auto() {
                    self.bcps.incr();
                }
            }
            0xff6a => self.ocps.sb(0xff6a, v),
            0xff6b => {
                let r = self.ocps.addr() as usize >> 3;
                let c = self.ocps.addr() as usize >> 1 & 0x03;
                if self.ocps.addr() & 0x01 == 0x00 {
                    self.ocpd[r][c][0] = v & 0x1f;
                    self.ocpd[r][c][1] = (self.ocpd[r][c][1] & 0x18) | (v >> 5);
                } else {
                    self.ocpd[r][c][1] = (self.ocpd[r][c][1] & 0x07) | ((v & 0x03) << 3);
                    self.ocpd[r][c][2] = (v >> 2) & 0x1f;
                }
                if self.ocps.auto() {
                    self.ocps.incr();
                }
            }
            _ => unreachable!(),
        }
    }
}

impl Ticker for Gpu {
    fn tick(&mut self, cycles: u16) {
        // The LCD controller operates on a 222 Hz = 4.194 MHz dot clock. An entire frame is 154 scanlines, 70224 dots,
        // or 16.74 ms. On scanlines 0 through 143, the LCD controller cycles through modes 2, 3, and 0 once every 456
        // dots. Scanlines 144 through 153 are mode 1.
        //
        // 1 scanline = 456 dots
        //
        // The following are typical when the display is enabled:
        // Mode 2  2_____2_____2_____2_____2_____2___________________2____
        // Mode 3  _33____33____33____33____33____33__________________3___
        // Mode 0  ___000___000___000___000___000___000________________000
        // Mode 1  ____________________________________11111111111111_____
        // When LCD is disabled the PPU is completely halted; dots and LY freeze.
        if !self.lcdc.bit7() {
            return;
        }
        if cycles == 0 {
            return;
        }
        let c = (cycles - 1) / 80 + 1;
        for i in 0..c {
            if i == (c - 1) {
                self.dots += cycles as u32 % 80
            } else {
                self.dots += 80
            }
            let d = self.dots;
            self.dots %= 456;
            if d == 452 {
                // LY increments 4T before the end of the scanline. The STAT LYC=LY bit is cleared immediately: it will
                // be re-evaluated when the new scanline starts (dots=0), matching DMG hardware behaviour.
                self.ly = (self.ly + 1) % 154;
                self.stat.lyeq(false);
                // Update the STAT IRQ signal. The LYC=LY source just went inactive. The new LY's LYC coincidence is
                // re-evaluated at mode-2 start (dots=0).
                self.sigq_update();
                continue;
            }
            // Mode 1: VBlank (LY 144-153).
            if self.ly >= 144 {
                if self.stat.mode() == 1 {
                    continue;
                }
                self.stat.data = (self.stat.data & !0x03) | 1;
                self.stat.lyeq(self.ly == self.lc);
                self.sigv.set();
                Interrupt::owned(self.glo.clone()).raise(InterruptFlag::VBlank);
                // DMG quirk: at line 144, a Mode 2 OAM pulse is generated simultaneously
                // with Mode 1 (VBlank) entry.  If the M2 interrupt is enabled and the
                // STAT signal was LOW, fire a rising edge now (before sigq_update
                // sets the persistent level based on Mode 1 / LYC).
                if self.stat.bit5() && self.sigq == 0x00 {
                    Interrupt::owned(self.glo.clone()).raise(InterruptFlag::LCD);
                }
                self.sigq_update();
                continue;
            }
            // Mode 2: OAM search (dots 0-79).
            if self.dots < 80 {
                // Line 0 after LCD enable: ly=0 and mode=0 (cleared at LCD-off) uniquely identify this case. Stay in
                // Mode 0, skip Mode 2 entirely.
                if self.stat.mode() == 0 && self.ly == 0 {
                    continue;
                }
                if self.stat.mode() == 2 {
                    continue;
                }
                self.stat.data = (self.stat.data & !0x03) | 2;
                self.stat.lyeq(self.ly == self.lc);
                // Compute sprite timing penalty for this scanline's mode3 window.
                self.pena = self.pena_calc() / 4 * 4;
                self.sigq_update();
                continue;
            }
            // Mode 3: Pixel transfer (dots 80-455, but may end early due to sprite penalty).
            if self.dots <= (80 + 172 + ((self.sx as u32 % 8 + 3) / 4) * 4) - 4 + self.pena {
                self.stat.data = (self.stat.data & !0x03) | 3;
                // Mode 3 has no STAT interrupt source; the signal may fall to LOW here unless LYC=LY keeps it high.
                // Update to track any falling edge.
                self.sigq_update();
                continue;
            }
            if self.stat.mode() == 0 {
                continue;
            }
            self.stat.data &= !0x03;
            self.sigh.set();
            self.sigq_update();
            // Render scanline.
            if self.glo.borrow().term == Term::CGB || self.lcdc.bit0() {
                self.draw_bg();
            }
            if self.lcdc.bit1() {
                self.draw_sprites();
            }
        }
    }
}
