pub(crate) const DEFAULT_PORT: u16 = 47842;
pub(crate) const STREAM_BUFFER_SIZE: usize = 100;

pub(crate) enum ProgramRole {
    Server,
    Client,
}

pub(crate) struct ProgramOptions {
    pub(crate) program_role: ProgramRole,
    pub(crate) server_ip_address: String,
	pub(crate) port: u16,
}
