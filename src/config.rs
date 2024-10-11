use crate::cli::Profile;
use crate::context::{ContextBuilder, IntelHdaType, Vga};

// Look at the readme for setup instructions. The builder functions also have doc comments.

pub fn get_builder(_window: bool, _profile: &Profile) -> ContextBuilder {
	ContextBuilder::default()
		.smp("sockets=1,cores=4,threads=2")
		.ram("16G")
		.virtio_disk("/dev/disk/by-id/nvme-WD_BLACK_SN770M_1TB_24102W800941") // the block device you installed Windows on
		.ovmf_bios("/usr/share/OVMF", "/usr/share/qemu/OVMF.fd")
		.window()
		.vfio_user_networking()
		.pipewire("/run/user/1000") // your UID
		.intel_hda(IntelHdaType::Output)
		.vga(Vga::Standard)
		.usb_tablet()
		.spice_kvm()
		.spice_agent()
	//.usb_device(0x046d, 0xc547)
}
