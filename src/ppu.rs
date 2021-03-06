use std::fmt;

use addressable::{PpuAddressable, Address, Result, Error, AddressDiff};
use cpu::InterruptState;
use renderer::Frame;

const CONTROLLER: Address = 0;
const MASK: Address = 1;
const STATUS: Address = 2;
const OAM_ADDRESS: Address = 3;
const OAM_DATA: Address = 4;
const SCROLL: Address = 5;
const ADDRESS: Address = 6;
const DATA: Address = 7;

const CONTROLLER_BASE_NAMETABLE_ADDRESS_MASK: u8 = mask!(2);
const CONTROLLER_NAMETABLE_X: u8 = bit!(0);
const CONTROLLER_NAMETABLE_Y: u8 = bit!(1);
const CONTROLLER_VRAM_ADDRESS_INCREMENT: u8 = bit!(2);
const CONTROLLER_SPRITE_PATTERN_TABLE_8X8: u8 = bit!(3);
const CONTROLLER_BACKGROUND_PATTERN_TABLE: u8 = bit!(4);
const CONTROLLER_SPRITE_SIZE: u8 = bit!(5);
const CONTROLLER_PPU_MASTER_SLAVE_SELECT: u8 = bit!(6);
const CONTROLLER_VBLANK_NMI: u8 = bit!(7);

const MASK_GREYSCALE: u8 = bit!(0);
const MASK_BACKGROUND_LEFT: u8 = bit!(1);
const MASK_SPRITES_LEFT: u8 = bit!(2);
const MASK_BACKGROUND: u8 = bit!(3);
const MASK_SPRITES: u8 = bit!(4);
const MASK_EMPHASIZE_RED: u8 = bit!(5);
const MASK_EMPHASIZE_GREEN: u8 = bit!(6);
const MASK_EMPHASIZE_BLUE: u8 = bit!(7);

const STATUS_LAST_WRITE_MASK: u8 = mask!(5);
const STATUS_SPRITE_OVERFLOW: u8 = bit!(5);
const STATUS_SPRITE_0_HIT: u8 = bit!(6);
const STATUS_VBLANK: u8 = bit!(7);

const OAM_SIZE: usize = 0x100;
const NAMETABLE_SIZE: AddressDiff = 0x400;
const NAMETABLE_OFFSET: AddressDiff = 0x2000;

pub const DISPLAY_WIDTH: usize = 256;
pub const DISPLAY_HEIGHT: usize = 240;
pub const NUM_PIXELS: usize = DISPLAY_WIDTH * DISPLAY_HEIGHT;

pub const WIDTH_TILES: AddressDiff = 32;
pub const HEIGHT_TILES: AddressDiff = 30;
pub const TILE_WIDTH: AddressDiff = 8;
pub const TILE_HEIGHT: AddressDiff = 8;
pub const PATTERN_TABLE_ENTRY_BYTES: AddressDiff = 16;
pub const ATTRIBUTE_TABLE_OFFSET: AddressDiff = 0x3c0;

pub const PALETTE_STRIDE: AddressDiff = 4;
pub const UNIVERSAL_BACKGROUND_COLOUR: Address = 0x3f00;
pub const BACKGROUND_PALETTE_BASE: Address = 0x3f00;
pub const SPRITE_PALETTE_BASE: Address = 0x3f10;

pub const SPRITE_STRIDE: usize = 4;
pub const NUM_SPRITES: usize = 64;

const SPRITE_ATTRIBUTE_PALETTE_MASK: u8 = mask!(2);
const SPRITE_ATTRIBUTE_PRIORITY: u8 = bit!(5);
const SPRITE_ATTRIBUTE_HORIZONTAL_FLIP: u8 = bit!(6);
const SPRITE_ATTRIBUTE_VERTICAL_FLIP: u8 = bit!(7);

const TILE_SIZE_BITS: AddressDiff = 3;
const SUBTILE_OFFSET_MASK: AddressDiff = mask!(TILE_SIZE_BITS);
const TILE_COORD_MASK: AddressDiff = !SUBTILE_OFFSET_MASK;

enum ScrollAxis { X, Y }
enum AddressPhase { LOW, HIGH }

pub struct PpuRegisterFile {
    controller: u8,
    mask: u8,
    status: u8,
    oam_address: u8,
    scroll: u8,
    address: u8,
}

#[derive(Debug)]
struct Sprite {
    x: u8,
    y: u8,
    index: u8,
    palette: u8,
    priority: bool,
    horizontal_flip: bool,
    vertical_flip: bool,
}

impl Sprite {
    fn new(x: u8, y: u8, attributes: u8, index: u8) -> Self {
        Sprite {
            x: x,
            y: y,
            index: index,
            palette: attributes & SPRITE_ATTRIBUTE_PALETTE_MASK,
            priority: attributes & SPRITE_ATTRIBUTE_PRIORITY != 0,
            horizontal_flip: attributes & SPRITE_ATTRIBUTE_HORIZONTAL_FLIP != 0,
            vertical_flip: attributes & SPRITE_ATTRIBUTE_VERTICAL_FLIP != 0,
        }
    }

    fn is_visible(&self) -> bool {
        self.y < 0xef
    }
}

impl fmt::Display for PpuRegisterFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        try!(writeln!(f, "PPUCTRL: {:02x}", self.controller));
        try!(writeln!(f, "PPUMASK: {:02x}", self.mask));
        try!(writeln!(f, "PPUSTATUS: {:02x}", self.status));
        try!(writeln!(f, "OAMADDR: {:02x}", self.oam_address));
        Ok(())
    }
}
impl PpuRegisterFile {
    fn new() -> Self {
        PpuRegisterFile {
            controller: 0,
            mask: 0,
            status: 0,
            oam_address: 0,
            scroll: 0,
            address: 0,
        }
    }
}

pub struct Ppu {
    pub registers: PpuRegisterFile,
    scroll_axis: ScrollAxis,
    scroll_x: u8,
    scroll_y: u8,
    address_phase: AddressPhase,
    address: Address,
    oam: Vec<u8>,
    data_latch: u8,
}

impl fmt::Display for Ppu {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        try!(writeln!(f, "scroll: [ x: {}, y: {} ]", self.scroll_x, self.scroll_y));
        try!(writeln!(f, "address: {:04x}", self.address));
        try!(write!(f, "registers:\n{}", self.registers));
        try!(writeln!(f, "OAM:"));
        let mut address = 0;
        loop {
            try!(write!(f, "0x{:02x}:", address));
            for _ in 0..16 {
                try!(write!(f, " {:02x}", self.oam[address]));
                address += 1;
            }
            try!(writeln!(f, ""));
            if address == OAM_SIZE {
                break;
            }
        }
        Ok(())
    }
}

impl Ppu {
    pub fn new() -> Self {
        Ppu {
            registers: PpuRegisterFile::new(),
            scroll_axis: ScrollAxis::X,
            scroll_x: 0,
            scroll_y: 0,
            address_phase: AddressPhase::HIGH,
            address: 0,
            oam: vec![0; OAM_SIZE],
            data_latch: 0,
        }
    }

    pub fn vblank_start(&mut self, mut interrupts: InterruptState) -> InterruptState {
        self.registers.status |= STATUS_VBLANK;

        if self.registers.controller & CONTROLLER_VBLANK_NMI != 0 {
            interrupts.nmi = true;
        }

        interrupts
    }

    pub fn vblank_end(&mut self, interrupts: InterruptState) -> InterruptState {
        self.registers.status &= !STATUS_VBLANK;
        interrupts
    }

    pub fn render_end(&mut self) {
        self.registers.status &= !STATUS_SPRITE_0_HIT;
    }

    pub fn set_oam_address(&mut self, address: u8) {
        self.registers.oam_address = address;
    }

    pub fn oam_data_write(&mut self, data: u8) {
        self.oam[self.registers.oam_address as usize] = data;
        self.registers.oam_address = self.registers.oam_address.wrapping_add(1);
    }

    fn increment_address(&mut self) {
        if self.registers.controller & CONTROLLER_VRAM_ADDRESS_INCREMENT != 0 {
            self.address = self.address.wrapping_add(32);
        } else {
            self.address = self.address.wrapping_add(1);
        }
    }

    pub fn read8<Memory: PpuAddressable>(&mut self, address: Address, mut memory: Memory) -> Result<u8> {
        let data = match address {
            CONTROLLER => return Err(Error::IllegalRead(address)),
            MASK => return Err(Error::IllegalRead(address)),
            STATUS => {
                let value = self.registers.status;
                self.registers.status &= !STATUS_VBLANK;
                value
            }
            OAM_ADDRESS => return Err(Error::IllegalRead(address)),
            OAM_DATA => self.oam[self.registers.oam_address as usize],
            SCROLL => return Err(Error::IllegalRead(address)),
            ADDRESS => return Err(Error::IllegalRead(address)),
            DATA => {
                let data = self.data_latch;
                self.data_latch = try!(memory.ppu_read8(self.address));
                self.increment_address();
                data
            }
            _ => return Err(Error::UnimplementedRead(address)),
        };

        Ok(data)
    }

    pub fn write8<Memory: PpuAddressable>(&mut self, address: Address, data: u8, mut memory: Memory) -> Result<()> {
        self.registers.status |= data & STATUS_LAST_WRITE_MASK;

        match address {
            CONTROLLER => self.registers.controller = data,
            MASK => self.registers.mask = data,
            STATUS => return Err(Error::IllegalWrite(address)),
            OAM_ADDRESS => self.set_oam_address(data),
            OAM_DATA => self.oam_data_write(data),
            SCROLL => {
                match self.scroll_axis {
                    ScrollAxis::X => self.scroll_axis = ScrollAxis::Y,
                    ScrollAxis::Y => {
                        self.scroll_axis = ScrollAxis::X;
                        self.scroll_x = self.registers.scroll;
                        self.scroll_y = data;
                    }
                }
                self.registers.scroll = data;
            }
            ADDRESS => {
                match self.address_phase {
                    AddressPhase::HIGH => {
                        self.address_phase = AddressPhase::LOW;
                    }
                    AddressPhase::LOW => {
                        self.address_phase = AddressPhase::HIGH;
                        self.address = ((self.registers.address as u16) << 8) | (data as u16);
                    }
                }
                self.registers.address = data;
            }
            DATA => {
                try!(memory.ppu_write8(self.address, data));
                self.increment_address();
            }
            _ => return Err(Error::UnimplementedWrite(address)),
        }
        Ok(())
    }

    fn background_base_patterntable_address(&self) -> Address {
        if self.registers.controller & CONTROLLER_BACKGROUND_PATTERN_TABLE == 0 {
            0x0000
        } else {
            0x1000
        }
    }

    fn background_top_left_coord(&self) -> (AddressDiff, AddressDiff) {
        let mut x = self.scroll_x as AddressDiff;
        let mut y = self.scroll_y as AddressDiff;

        if self.registers.controller & CONTROLLER_NAMETABLE_X != 0 {
            x += DISPLAY_WIDTH as AddressDiff;
        }
        if self.registers.controller & CONTROLLER_NAMETABLE_Y != 0 {
            y += DISPLAY_HEIGHT as AddressDiff;
        }

        (x, y)
    }

    fn sprite_base_patterntable_address(&self) -> Address {
        if self.registers.controller & CONTROLLER_SPRITE_PATTERN_TABLE_8X8 == 0 {
            0x0000
        } else {
            0x1000
        }
    }


    fn metatile_id(tile_x: AddressDiff, tile_y: AddressDiff) -> u8 {
        // a metatile is 2x2 tiles
        let x = tile_x / 2;
        let y = tile_y / 2;

        // ids are unique within a 4x4 tile block
        (((y & bit!(0)) << 1) | (x & bit!(0))) as u8
    }

    fn render_background_tile<F: Frame, M: PpuAddressable>(&mut self,
                                                           frame: &mut F,
                                                           memory: &mut M,
                                                           pt_base: Address,
                                                           nt_base: Address,
                                                           nt_tile_x: AddressDiff,
                                                           nt_tile_y: AddressDiff,
                                                           px_off_x: isize,
                                                           px_off_y: isize) -> Result<()> {

        let nt_offset = nt_tile_y * WIDTH_TILES + nt_tile_x;
        let nt_address = nt_base + nt_offset;
        let pt_index = try!(memory.ppu_read8(nt_address)) as AddressDiff;
        let pt_offset = pt_index * PATTERN_TABLE_ENTRY_BYTES;
        let pt_address = pt_base | pt_offset;

        let at_base = nt_base + ATTRIBUTE_TABLE_OFFSET;
        let at_index = (nt_tile_y / 4) * (WIDTH_TILES / 4) + (nt_tile_x / 4);
        let at_byte_address = at_base + at_index;
        let at_byte = try!(memory.ppu_read8(at_byte_address));

        // 2 bits per entry
        let at_bits = (at_byte >> (Self::metatile_id(nt_tile_x, nt_tile_y) * 2)) & mask!(2);

        let palette_base = BACKGROUND_PALETTE_BASE + (at_bits as AddressDiff * PALETTE_STRIDE);

        for i in 0..TILE_HEIGHT {
            let mut row_0 = try!(memory.ppu_read8(pt_address + i));
            let mut row_1 = try!(memory.ppu_read8(pt_address + TILE_HEIGHT + i));

            let pixel_y = px_off_y + i as isize;

            if pixel_y < 0 || pixel_y >= DISPLAY_HEIGHT as isize {
                continue;
            }

            for j in 0..TILE_WIDTH {
                let palette_index = (row_0 & bit!(0)) | ((row_1 & bit!(0)) << 1);
                row_0 >>= 1;
                row_1 >>= 1;

                if palette_index != 0 {
                    let palette_address = palette_base + palette_index as AddressDiff;
                    let colour = try!(memory.ppu_read8(palette_address));

                    let pixel_x_offset = (TILE_WIDTH - 1 - j) as isize;
                    let pixel_x = px_off_x + pixel_x_offset;

                    if pixel_x >= 0 && pixel_x < DISPLAY_WIDTH as isize {
                        frame.set_pixel(pixel_x as usize, pixel_y as usize, colour);
                    }
                }
            }
        }
        Ok(())
    }

    fn render_universal_background<F: Frame, M: PpuAddressable>(&mut self, frame: &mut F, memory: &mut M) -> Result<()> {
        let colour = try!(memory.ppu_read8(UNIVERSAL_BACKGROUND_COLOUR));
        for i in 0..DISPLAY_HEIGHT {
            for j in 0..DISPLAY_WIDTH {
                frame.set_pixel(j, i, colour);
            }
        }
        Ok(())
    }

    // returns (nametable_start_address, nametable_offset)
    fn tile_coord_to_nametable_base(&self, x: AddressDiff, y: AddressDiff) -> AddressDiff {
        if x < WIDTH_TILES {
            if y < HEIGHT_TILES {
                0x2000
            } else {
                0x2800
            }
        } else {
            if y < HEIGHT_TILES {
                0x2400
            } else {
                0x2c00
            }
        }
    }

    fn render_background<F: Frame, M: PpuAddressable>(&mut self, frame: &mut F, memory: &mut M) -> Result<()> {
        let pt_base = self.background_base_patterntable_address();

        let (top_left_pixel_x, top_left_pixel_y) = self.background_top_left_coord();

        let pixel_offset_x = (top_left_pixel_x & SUBTILE_OFFSET_MASK) as isize;
        let pixel_offset_y = (top_left_pixel_y & SUBTILE_OFFSET_MASK) as isize;

        let tile_offset_x = top_left_pixel_x >> TILE_SIZE_BITS;
        let tile_offset_y = top_left_pixel_y >> TILE_SIZE_BITS;

        for i in 0..(HEIGHT_TILES + 1) {
            let abs_i = (i + tile_offset_y) % (HEIGHT_TILES * 2);
            for j in 0..(WIDTH_TILES + 1) {
                let abs_j = (j + tile_offset_x) % (WIDTH_TILES * 2);

                let nametable_address = self.tile_coord_to_nametable_base(abs_j, abs_i);

                let local_x = abs_j % WIDTH_TILES;
                let local_y = abs_i % HEIGHT_TILES;

                let px_x = (j * TILE_WIDTH) as isize - pixel_offset_x;
                let px_y = (i * TILE_HEIGHT) as isize - pixel_offset_y;

                try!(self.render_background_tile(frame, memory, pt_base, nametable_address,
                                            local_x, local_y, px_x, px_y));
            }
        }

        Ok(())
    }

    fn render_sprite_8x8<F: Frame, M: PpuAddressable>(&mut self, frame: &mut F, memory: &mut M, sprite: Sprite) -> Result<bool> {

        let mut hit = false;

        let pt_base = self.sprite_base_patterntable_address();
        let pt_offset = sprite.index as AddressDiff * PATTERN_TABLE_ENTRY_BYTES;
        let pt_address = pt_base | pt_offset;

        let palette_base = SPRITE_PALETTE_BASE + sprite.palette as AddressDiff * PALETTE_STRIDE;

        for i in 0..TILE_HEIGHT {
            let mut row_0 = try!(memory.ppu_read8(pt_address + i));
            let mut row_1 = try!(memory.ppu_read8(pt_address + TILE_HEIGHT + i));

            let pixel_y = if sprite.vertical_flip {
                sprite.y as AddressDiff + TILE_HEIGHT - 1 - i
            } else {
                sprite.y as AddressDiff + i
            };

            for j in 0..TILE_WIDTH {
                let palette_index = (row_0 & bit!(0)) | ((row_1 & bit!(0)) << 1);
                row_0 >>= 1;
                row_1 >>= 1;

                if palette_index != 0 {
                    let palette_address = palette_base + palette_index as AddressDiff;
                    let colour = try!(memory.ppu_read8(palette_address));

                    let pixel_x = if sprite.horizontal_flip {
                        sprite.x as AddressDiff + j
                    } else {
                        sprite.x as AddressDiff + TILE_WIDTH - 1 - j
                    };

                    frame.set_pixel(pixel_x as usize, pixel_y as usize, colour);
                    hit = true;
                }
            }
        }

        Ok(hit)
    }

    fn render_sprites_8x8<F: Frame, M: PpuAddressable>(&mut self, frame: &mut F, memory: &mut M) -> Result<()> {

        for i in 0..NUM_SPRITES {
            let index = i * SPRITE_STRIDE;
            let sprite = Sprite::new(self.oam[index + 3],
                                     self.oam[index + 0],
                                     self.oam[index + 2],
                                     self.oam[index + 1]);

            if sprite.is_visible() {
                let hit = try!(self.render_sprite_8x8(frame, memory, sprite));
                if i == 0 && hit {
                    self.registers.status |= STATUS_SPRITE_0_HIT;
                }
            }
        }

        Ok(())
    }

    pub fn render<F: Frame, M: PpuAddressable>(&mut self, frame: &mut F, memory: &mut M) -> Result<()> {
        try!(self.render_universal_background(frame, memory));
        try!(self.render_background(frame, memory));
        try!(self.render_sprites_8x8(frame, memory));
        Ok(())
    }
}
