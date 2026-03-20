pub enum SyncCommand {
	Share(Vec<u8>),
	Receive(String),
	Cancel,
}

pub enum SyncResult {
	TicketGenerated(String),
	DataReceived(Vec<u8>),
	Error(String),
}
