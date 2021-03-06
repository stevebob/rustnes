use addressable::{PpuAddressable, Address, AddressDiff, Result, Error};
use cartridge::{Cartridge, NAME_TABLE_START};
use vram::NesVram;
use palette::Palette;

const CARTRIDGE_START: Address = 0x0000;
const CARTRIDGE_END: Address = 0x2fff;
const NAME_TABLE_MIRROR_START: Address = 0x3000;
const NAME_TABLE_MIRROR_END: Address = 0x3eff;
const PALETTE_START: Address = 0x3f00;
const PALETTE_END: Address = 0x3f1f;
const PALETTE_MIRROR_START: Address = 0x3f20;
const PALETTE_MIRROR_END: Address = 0x3fff;

const NAME_TABLE_MIRROR_OFFSET: AddressDiff = NAME_TABLE_MIRROR_START - NAME_TABLE_START;
const PALETTE_SIZE: AddressDiff = PALETTE_END - PALETTE_START + 1;

pub struct PpuMemoryLayout<'a, C: 'a + Cartridge> {
    cartridge: &'a mut C,
    vram: &'a mut NesVram,
    palette: &'a mut Palette,
}

impl<'a, C: 'a + Cartridge> PpuMemoryLayout<'a, C> {
    pub fn new(cartridge: &'a mut C, vram: &'a mut NesVram, palette: &'a mut Palette) -> Self {
        PpuMemoryLayout {
            cartridge: cartridge,
            vram: vram,
            palette: palette,
        }
    }
}

impl<'a, C: 'a + Cartridge> PpuAddressable for PpuMemoryLayout<'a, C> {
    fn ppu_read8(&mut self, address: Address) -> Result<u8> {
        match address {
            CARTRIDGE_START...CARTRIDGE_END => self.cartridge.ppu_read8(address, self.vram),
            NAME_TABLE_MIRROR_START...NAME_TABLE_MIRROR_END => {
                self.cartridge.ppu_read8(address - NAME_TABLE_MIRROR_OFFSET, self.vram)
            }
            PALETTE_START...PALETTE_END => self.palette.ppu_read8(address - PALETTE_START),
            PALETTE_MIRROR_START...PALETTE_MIRROR_END => self.palette.ppu_read8((address - PALETTE_MIRROR_START) % PALETTE_SIZE),
            _ => Err(Error::BusErrorRead(address)),
        }
    }

    fn ppu_write8(&mut self, address: Address, data: u8) -> Result<()> {
        match address {
            CARTRIDGE_START...CARTRIDGE_END => self.cartridge.ppu_write8(address, data, self.vram),
            NAME_TABLE_MIRROR_START...NAME_TABLE_MIRROR_END => {
                self.cartridge.ppu_write8(address - NAME_TABLE_MIRROR_OFFSET, data, self.vram)
            }
            PALETTE_START...PALETTE_END => self.palette.ppu_write8(address - PALETTE_START, data),
            PALETTE_MIRROR_START...PALETTE_MIRROR_END => self.palette.ppu_write8((address - PALETTE_MIRROR_START) % PALETTE_SIZE, data),
            _ => Err(Error::BusErrorWrite(address)),
        }
    }
}
