use addressable::{CpuAddressable, Address, Result, Error};

pub struct NesIoPorts {}

impl NesIoPorts {
    pub fn new() -> Self {
        NesIoPorts {}
    }
}

impl CpuAddressable for NesIoPorts {
    fn read(&mut self, address: Address) -> Result<u8> {
        Err(Error::UnimplementedRead(address))
    }

    fn write(&mut self, address: Address, _: u8) -> Result<()> {
        Err(Error::UnimplementedWrite(address))
    }
}