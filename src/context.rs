use crate::util::{ArgWriter, EnvWriter, TmpFileWriter};
use nix::sys::stat::Mode;
use nix::unistd::{Gid, Uid};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct UsbAddress {
	vendor_id: u16,
	product_id: u16,
}

#[derive(Debug)]
pub struct TmpFile {
	pub path: PathBuf,
	pub uid: Uid,
	pub gid: Gid,
	pub mode: Mode,
}

#[derive(Debug)]
enum Graphics {
	None,
	/// Virtio VGA device with GTK interface. Requires running display server.
	Virtio,
}

#[derive(Debug)]
enum BiosType {
	Default,
	/// Boot in UEFI mode. Argument is the path to OVMF.fd; Install edk2-ovmf on arch.
	Ovmf(PathBuf),
}

#[derive(Debug)]
enum Disk {
	/// Compatible with Windows out-of-the box, but slow performance.
	Raw(PathBuf),
	/// Faster, but requires driver installation on guest.
	Virtio(PathBuf),
}

#[derive(Debug)]
enum Audio {
	None,
	/// Argument is the run dir, typically /run/user/$UID
	Pipewire(PathBuf),
}

#[derive(Debug)]
enum Networking {
	None,
	/// Compatible with Windows out-of-the box, but high CPU overhead and wonky performance.
	User,
	/// Less overhead, more stable, but requires driver installation on guest.
	VfioUser,
}

#[derive(Debug)]
enum LookingGlass {
	No,
	Yes(Uid, Gid),
}

#[derive(Debug)]
pub struct Context {
	pub env: HashMap<String, String>,
	pub args: Vec<String>,
	pub pci: Vec<String>,
	pub tmp_files: Vec<TmpFile>,
	pub cpu_affinity: Option<String>,
	pub unload_drivers: Option<Vec<String>>,
}

#[derive(Debug)]
pub struct ContextBuilder {
	cpu: Option<String>,
	smp: Option<String>,
	ram: String,
	bios_type: BiosType,
	graphics: Graphics,
	audio: Audio,
	networking: Networking,
	looking_glass: LookingGlass,
	disks: Vec<Disk>,
	pci: Vec<String>,
	unload_drivers: Option<Vec<String>>,
	usb: Vec<UsbAddress>,
	cpu_affinity: Option<String>,
}

impl Default for ContextBuilder {
	fn default() -> Self {
		Self {
			cpu: None,
			smp: None,
			ram: String::from("4G"),
			bios_type: BiosType::Default,
			graphics: Graphics::None,
			audio: Audio::None,
			networking: Networking::None,
			looking_glass: LookingGlass::No,
			disks: vec![],
			pci: vec![],
			usb: vec![],
			unload_drivers: None,
			cpu_affinity: None,
		}
	}
}

impl ContextBuilder {
	pub fn with_cpu(mut self, options: impl Into<String>) -> Self {
		self.cpu = Some(options.into());
		self
	}

	pub fn with_cpu_affinity(mut self, affinity: impl Into<String>) -> Self {
		self.cpu_affinity = Some(affinity.into());
		self
	}

	pub fn with_smp(mut self, layout: impl Into<String>) -> Self {
		self.smp = Some(layout.into());
		self
	}

	pub fn with_ram(mut self, size: impl Into<String>) -> Self {
		self.ram = size.into();
		self
	}

	pub fn with_ovmf_bios(mut self, path: impl Into<PathBuf>) -> Self {
		self.bios_type = BiosType::Ovmf(path.into());
		self
	}

	pub fn with_vfio_disk(mut self, path: impl Into<PathBuf>) -> Self {
		self.disks.push(Disk::Virtio(path.into()));
		self
	}

	#[allow(unused)]
	pub fn with_raw_disk(mut self, path: impl Into<PathBuf>) -> Self {
		self.disks.push(Disk::Raw(path.into()));
		self
	}

	pub fn with_pipewire(mut self, runtime_dir: impl Into<PathBuf>) -> Self {
		self.audio = Audio::Pipewire(runtime_dir.into());
		self
	}

	pub fn with_vfio_user_networking(mut self) -> Self {
		self.networking = Networking::VfioUser;
		self
	}

	#[allow(unused)]
	pub fn with_user_networking(mut self) -> Self {
		self.networking = Networking::User;
		self
	}

	#[allow(unused)]
	pub fn with_usb_device(mut self, vendor_id: u16, product_id: u16) -> Self {
		self.usb.push(UsbAddress { vendor_id, product_id });
		self
	}

	pub fn with_pci_device(mut self, address: impl Into<String>) -> Self {
		self.pci.push(address.into());
		self
	}

	pub fn with_unloaded_drivers<T: Into<String>>(mut self, drivers: Vec<T>) -> Self {
		let drivers = drivers.into_iter().map(|d| d.into()).collect::<Vec<_>>();
		self.unload_drivers = Some(drivers);
		self
	}

	pub fn with_graphics(mut self) -> Self {
		self.graphics = Graphics::Virtio;
		self
	}

	pub fn with_looking_glass(mut self, owner: impl Into<Uid>, group: impl Into<Gid>) -> Self {
		self.looking_glass = LookingGlass::Yes(owner.into(), group.into());
		self
	}

	pub fn build(self) -> Context {
		let mut arg_writer = ArgWriter::default();
		let mut env_writer = EnvWriter::default();
		let mut tmp_file_writer = TmpFileWriter::default();

		add_defaults(&mut arg_writer);
		add_monitor(&mut arg_writer);
		add_system(&mut arg_writer, self.cpu, self.smp, self.ram);
		add_bios(&mut arg_writer, self.bios_type);
		add_graphics(&mut arg_writer, self.graphics);
		add_audio(&mut arg_writer, &mut env_writer, self.audio);
		add_networking(&mut arg_writer, self.networking);
		add_pci(&mut arg_writer, &self.pci);
		add_disks(&mut arg_writer, self.disks);
		add_usb(&mut arg_writer, self.usb);
		add_looking_glass(&mut arg_writer, &mut tmp_file_writer, self.looking_glass);

		Context {
			env: env_writer.get_envs(),
			args: arg_writer.get_args(),
			pci: self.pci,
			cpu_affinity: self.cpu_affinity,
			unload_drivers: self.unload_drivers,
			tmp_files: tmp_file_writer.get_tmp_files(),
		}
	}
}

fn add_defaults(args: &mut ArgWriter) {
	args.add_many(vec!["-nodefaults", "-enable-kvm"]);
}

fn add_monitor(args: &mut ArgWriter) {
	args.add_many(vec![
		"-mon",
		"chardev=char0,mode=readline",
		"-chardev",
		"stdio,id=char0,mux=on",
	]);
}

fn add_system(args: &mut ArgWriter, cpu: Option<String>, smp: Option<String>, ram: String) {
	if let Some(cpu) = cpu {
		args.add("-cpu").add(cpu);
	}

	if let Some(smp) = smp {
		args.add("-smp").add(smp);
	}

	args.add("-m").add(ram);
}

fn add_bios(args: &mut ArgWriter, bios: BiosType) {
	match bios {
		BiosType::Default => (),
		BiosType::Ovmf(path) => {
			let firmware_directory = path.parent().expect("bios file should be in a directory");

			args.add("-L")
				.add(firmware_directory.to_string_lossy())
				.add("-bios")
				.add(path.to_string_lossy());
		}
	}
}

fn add_graphics(args: &mut ArgWriter, graphics: Graphics) {
	match graphics {
		Graphics::None => args.add_many(vec!["-nographic", "-vga", "none"]),
		Graphics::Virtio => args.add_many(vec!["-vga", "virtio"]),
	};
}

fn add_audio(args: &mut ArgWriter, env: &mut EnvWriter, audio: Audio) {
	match audio {
		Audio::None => (),
		Audio::Pipewire(runtime_dir) => {
			env.add("PIPEWIRE_RUNTIME_DIR", runtime_dir.to_string_lossy())
				.add("PIPEWIRE_LATENCY", "128/48000");

			args.add_many(vec![
				"-audiodev",
				"pipewire,id=pw",
				"-device",
				"intel-hda",
				"-device",
				"hda-output,audiodev=pw,mixer=off",
			]);
		}
	}
}

fn add_networking(args: &mut ArgWriter, networking: Networking) {
	match networking {
		Networking::None => {
			args.add_many(vec!["-nic", "none"]);
		}
		Networking::User => {
			args.add_many(vec!["-nic", "model=e1000"]);
		}
		Networking::VfioUser => {
			args.add_many(vec!["-nic", "model=virtio-net-pci"]);
		}
	}
}

fn add_pci(args: &mut ArgWriter, devices: &[String]) {
	for address in devices.iter() {
		args.add("-device").add(format!("vfio-pci,host={address}"));
	}
}

fn add_disks(args: &mut ArgWriter, disks: Vec<Disk>) {
	for disk in disks.iter() {
		_ = match disk {
			Disk::Raw(device) => args.add("-drive").add(raw_disk(device, "media=disk")),
			Disk::Virtio(device) => args.add("-drive").add(raw_disk(device, "if=virtio")),
		};
	}

	fn raw_disk(device: &Path, options: &str) -> String {
		let dev = device.to_string_lossy();
		format!("file={dev},format=raw,{options}")
	}
}

fn add_usb(args: &mut ArgWriter, devices: Vec<UsbAddress>) {
	if devices.is_empty() {
		return;
	}

	args.add("-usb");

	for address in devices.iter() {
		let fmt = format!(
			"usb-host,vendorid={:x},productid={:x}",
			address.vendor_id, address.product_id
		);
		args.add("-device").add(fmt);
	}
}

fn add_looking_glass(args: &mut ArgWriter, tmp: &mut TmpFileWriter, config: LookingGlass) {
	let LookingGlass::Yes(uid, gid) = config else {
		return;
	};

	let mode = Mode::from_bits_truncate(0o644);
	tmp.add("/dev/shm/looking-glass", uid, gid, mode);

	args.add_many(vec![
		"-device",
		"ivshmem-plain,memdev=ivshmem,bus=pci.0",
		"-object",
		"memory-backend-file,id=ivshmem,share=on,mem-path=/dev/shm/looking-glass,size=32M",
	]);
}
