//! ISO-TP (ISO 15765-2) PCI 常量与默认值

// ============ PCI 常量 ============

pub const PCI_TYPE_MASK: u8 = 0xF0;
pub const PCI_SF: u8 = 0x00; // Single Frame
pub const PCI_FF: u8 = 0x10; // First Frame
pub const PCI_CF: u8 = 0x20; // Consecutive Frame
pub const PCI_FC: u8 = 0x30; // Flow Control

/// FC 帧的 FlowStatus 字段
pub const FC_CTS: u8 = 0x00; // Continue To Send
pub const FC_WAIT: u8 = 0x01; // Wait
pub const FC_OVERFLOW: u8 = 0x02; // Overflow

/// SF 最大数据长度 (经典 CAN 8 字节 - 1 PCI - 1 SF_DL = 7)
pub const SF_MAX_DATA: usize = 7;
/// FF 一次携带的数据长度 (8 - 2 字节 PCI = 6)
pub const FF_DATA_LEN: usize = 6;
/// CF 一次携带的数据长度 (8 - 1 字节 PCI = 7)
pub const CF_DATA_LEN: usize = 7;
/// FF_DL 最大值 (12 位)
pub const FF_DL_MAX: usize = 0xFFF;

/// 默认超时 (ISO 15765-2 推荐,单位 ms)
pub const DEFAULT_N_BS_MS: u64 = 1000; // 发送方等 FC
pub const DEFAULT_N_CR_MS: u64 = 1000; // 接收方等 CF
pub const DEFAULT_N_AS_MS: u64 = 100; // 发送方等 CAN ACK
pub const DEFAULT_N_AR_MS: u64 = 100; // 接收方等 CAN ACK

/// 提取 PCI 类型 (高 4 位)
#[inline]
pub const fn pci_type(first_byte: u8) -> u8 {
    first_byte & PCI_TYPE_MASK
}
