use crate::pci::PCI;

const HDA_PCI_CLASS: u8 = 0x04;
const HDA_PCI_SUBCLASS: u8 = 0x03;

const GCTL: u32 = 0x08;
const STATESTS: u32 = 0x0C;
const INTSTS: u32 = 0x0D08;
const INTCTL: u32 = 0x0D0C;
const WALLCLK: u32 = 0x0D10;
const SRCTL: u32 = 0x0D18;
const SRBAR: u32 = 0x0D20;
const CORBLBASE: u32 = 0x0D28;
const CORBUBASE: u32 = 0x0D2C;
const CORBWP: u32 = 0x0D30;
const CORBRP: u32 = 0x0D34;
const CORBCTL: u32 = 0x0D3C;
const RIRBWP: u32 = 0x0D48;
const RIRBCNT: u32 = 0x0D4C;
const RIRBCTL: u32 = 0x0D5C;
const RIRBSTS: u32 = 0x0D5D;
const DPLBASE: u32 = 0x0D70;
const DPUBASE: u32 = 0x0D74;
const CODEC_GET_PARAMETER: u32 = 0xF00;
const VERB_SET_STREAM_FORMAT: u32 = 0x200;
const VERB_SET_AMP_GAIN: u32 = 0xB00;
const VERB_SET_PIN_WIDGET_CONTROL: u32 = 0x707;
const VERB_SET_PIN_SENSE: u32 = 0xF09;
const VERB_GET_PIN_SENSE: u32 = 0xF09;
const VERB_SET_CONNECT_SEL: u32 = 0x701;
const VERB_SET_UNSOLICITED_RESPONSE: u32 = 0x708;
const VERB_GET_STREAM_FORMAT: u32 = 0xA00;
const VERB_GET_AMP_GAIN: u32 = 0xB00;
const VERB_SET_CONVERTER_FORMAT: u32 = 0x200;
const VERB_SET_CHANNEL_STREAMID: u32 = 0x706;
const CORB_ENTRY_COUNT: usize = 256;
const RIRB_ENTRY_COUNT: usize = 256;

#[derive(Copy, Clone, PartialEq)]
pub enum HdCodecType {
    Unknown,
    Audio,
    Modem,
}

pub struct HdCodec {
    pub address: u8,
    pub codec_type: HdCodecType,
    pub vendor_id: u32,
    pub revision_id: u8,
    pub subsystem_id: u32,
    pub widgets: [HdWidget; 16],
    pub widget_count: usize,
}

#[derive(Copy, Clone)]
pub struct HdWidget {
    pub nid: u8,
    pub widget_type: u8,
    pub pin_cap: u32,
    pub audio_cap: u32,
    pub amp_cap: u32,
    pub connection_count: u8,
    pub is_output: bool,
}

pub struct HdaController {
    mmio: *mut u8,
    codecs: [Option<HdCodec>; 4],
    codec_count: usize,
    corb: *mut u32,
    rirb: *mut u64,
    corb_phys: u64,
    rirb_phys: u64,
    corb_rp: u16,
    rirb_wp: u16,
}

static mut HDA: Option<HdaController> = None;

impl HdaController {
    pub fn new(mmio: *mut u8) -> Self {
        Self {
            mmio,
            codecs: [None, None, None, None],
            codec_count: 0,
            corb: core::ptr::null_mut(),
            rirb: core::ptr::null_mut(),
            corb_phys: 0,
            rirb_phys: 0,
            corb_rp: 0,
            rirb_wp: 0,
        }
    }

    unsafe fn mmio_read32(&self, offset: u32) -> u32 {
        (self.mmio.add(offset as usize) as *const u32).read_volatile()
    }

    unsafe fn mmio_write32(&self, offset: u32, val: u32) {
        (self.mmio.add(offset as usize) as *mut u32).write_volatile(val);
    }

    unsafe fn mmio_read8(&self, offset: u32) -> u8 {
        self.mmio.add(offset as usize).read_volatile()
    }

    unsafe fn mmio_write8(&self, offset: u32, val: u8) {
        self.mmio.add(offset as usize).write_volatile(val);
    }

    pub unsafe fn reset(&mut self) {
        let gctl = self.mmio_read32(GCTL);
        self.mmio_write32(GCTL, gctl & !1);
        for _ in 0..100000 { core::hint::spin_loop(); }
        self.mmio_write32(GCTL, gctl | 1);
        for _ in 0..100000 { core::hint::spin_loop(); }
    }

    pub unsafe fn init_corb_rirb(&mut self) -> bool {
        use crate::heap::krust_malloc;
        use crate::paging::krust_paging_get_phys;
        self.corb = krust_malloc((CORB_ENTRY_COUNT * 4) as u32) as *mut u32;
        self.rirb = krust_malloc((RIRB_ENTRY_COUNT * 8) as u32) as *mut u64;
        if self.corb.is_null() || self.rirb.is_null() { return false; }
        core::ptr::write_bytes(self.corb, 0, CORB_ENTRY_COUNT * 4);
        core::ptr::write_bytes(self.rirb, 0, RIRB_ENTRY_COUNT * 8);
        self.corb_phys = krust_paging_get_phys(self.corb as u64);
        self.rirb_phys = krust_paging_get_phys(self.rirb as u64);
        self.mmio_write32(CORBLBASE, self.corb_phys as u32);
        self.mmio_write32(CORBUBASE, (self.corb_phys >> 32) as u32);
        self.mmio_write32(DPLBASE, self.rirb_phys as u32);
        self.mmio_write32(DPUBASE, (self.rirb_phys >> 32) as u32);
        self.corb_rp = 0;
        self.rirb_wp = 0;
        self.mmio_write32(CORBLBASE, self.corb_phys as u32);
        self.mmio_write32(CORBUBASE, (self.corb_phys >> 32) as u32);
        self.mmio_write8(CORBCTL, 0x02);
        self.mmio_write32(DPLBASE, self.rirb_phys as u32);
        self.mmio_write32(DPUBASE, (self.rirb_phys >> 32) as u32);
        self.mmio_write8(RIRBCTL, 0x02);
        self.mmio_write16(CORBWP, 0xFFFF);
        self.mmio_write16(CORBRP, 0x0000);
        for _ in 0..10000 { core::hint::spin_loop(); }
        self.mmio_write16(CORBRP, 0x8000);
        for _ in 0..10000 { core::hint::spin_loop(); }
        self.mmio_write16(CORBRP, 0x0000);
        for _ in 0..10000 { core::hint::spin_loop(); }
        true
    }

    unsafe fn mmio_write16(&self, offset: u32, val: u16) {
        (self.mmio.add(offset as usize) as *mut u16).write_volatile(val);
    }

    unsafe fn send_verb(&mut self, codec_addr: u8, verb_id: u32, param: u32) -> u32 {
        let verb = ((codec_addr as u32) << 28) | ((verb_id & 0xFFF) << 8) | (param & 0xFF);
        let wp = self.corb_rp.wrapping_add(1) % CORB_ENTRY_COUNT as u16;
        *self.corb.add(wp as usize) = verb.to_be();
        self.mmio_write16(CORBWP, wp as u16);
        self.corb_rp = wp;
        for _ in 0..10000 { core::hint::spin_loop(); }
        let response = self.mmio_read32(INTSTS);
        let _ = response;
        0
    }

    pub unsafe fn detect_codecs(&mut self) {
        let state = self.mmio_read32(STATESTS);
        self.codec_count = 0;
        for i in 0..4u32 {
            if state & (1 << i) != 0 {
                if self.codec_count < 4 {
                    let mut codec = HdCodec {
                        address: i as u8,
                        codec_type: HdCodecType::Unknown,
                        vendor_id: 0,
                        revision_id: 0,
                        subsystem_id: 0,
                        widgets: [HdWidget {
                            nid: 0,
                            widget_type: 0,
                            pin_cap: 0,
                            audio_cap: 0,
                            amp_cap: 0,
                            connection_count: 0,
                            is_output: false,
                        }; 16],
                        widget_count: 0,
                    };
                    let verb_data = ((i as u32) << 28) | ((CODEC_GET_PARAMETER as u32) << 8) | 0x00;
                    self.send_verb_raw(verb_data);
                    self.send_verb(i as u8, 0xF00, 0x00);
                    self.send_verb(i as u8, 0xF00, 0x04);
                    self.send_verb(i as u8, 0xF00, 0x09);
                    self.send_verb(i as u8, 0xF00, 0x0A);
                    codec.codec_type = HdCodecType::Audio;
                    self.codecs[self.codec_count] = Some(codec);
                    self.codec_count += 1;
                }
            }
        }
    }

    unsafe fn send_verb_raw(&mut self, data: u32) {
        let wp = self.corb_rp.wrapping_add(1) % CORB_ENTRY_COUNT as u16;
        *self.corb.add(wp as usize) = data.to_be();
        self.mmio_write16(CORBWP, wp as u16);
        self.corb_rp = wp;
        for _ in 0..10000 { core::hint::spin_loop(); }
    }

    pub unsafe fn init(&mut self) -> bool {
        self.reset();
        if !self.init_corb_rirb() { return false; }
        self.detect_codecs();
        true
    }

    pub fn codec_count(&self) -> usize { self.codec_count }
}

pub fn init_hda() -> bool {
    unsafe {
        if let Some(dev) = PCI::enumerate_class(HDA_PCI_CLASS, HDA_PCI_SUBCLASS) {
            let bar0 = PCI::config_read_dword(dev.bus, dev.slot, dev.func, 0x10);
            let bar4 = PCI::config_read_dword(dev.bus, dev.slot, dev.func, 0x20);
            if bar0 == 0 || bar4 == 0 { return false; }
            let mmio_phys = (bar0 & 0xFFFFFFF0) as *mut u8;
            let _ = bar4;
            PCI::enable_bus_mastering(dev.bus, dev.slot, dev.func);
            let mut controller = HdaController::new(mmio_phys);
            if controller.init() {
                crate::vga::krust_vga_writestring(b"HDA: initialized, codecs: \0" as *const u8);
                let mut buf = [0u8; 4];
                let count = controller.codec_count();
                if count == 0 {
                    buf[0] = b'0';
                } else {
                    buf[0] = b'0' + count as u8;
                }
                crate::vga::krust_vga_writestring(buf.as_ptr());
                crate::vga::krust_vga_writestring(b"\n\0" as *const u8);
                HDA = Some(controller);
                return true;
            }
        }
        false
    }
}

pub unsafe fn hda_beep(freq: u32, duration_ms: u32) {
    if let Some(ref mut _hda) = HDA {
        if freq == 0 { return; }
        let _ = freq;
        let _ = duration_ms;
    }
}
