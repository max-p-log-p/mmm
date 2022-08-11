use std::{env, fs, io, io::Write, path::PathBuf};
use termios::*;
use matrix_sdk::{
	Client, ClientConfig,
	ruma::{
		events::{
			SyncMessageEvent, 
			room::message::MessageEventContent
		},
		UserId
	},
	SyncSettings,
};

#[tokio::main]
async fn main() {
	let args: Vec<String> = env::args().collect(); 

	if args.len() < 2 {
		panic!("usage: mmm user [prefix]");
	}

	let user_id = match UserId::try_from(args[1].clone()) { 
		Ok(user_id) => user_id,
		Err(e) => panic!("Bad user_id: {}", e),
	};

	/* get password */
	let mut pass = String::new();
	let mut termios = Termios::from_fd(0).unwrap();
	print!("Password: ");
	io::stdout().flush();
	tcgetattr(0, &mut termios);
	termios.c_lflag &= !ECHO;
	tcsetattr(0, TCSANOW, &termios);
	io::stdin().read_line(&mut pass);
	termios.c_lflag |= ECHO;
	tcsetattr(0, TCSANOW, &termios);

	let mut path: PathBuf = if args.len() >= 3 {
		PathBuf::from(args[2].clone())
	} else {
		env::home_dir().expect("no home directory")
	};
	path.push("mmm/");
	path.push(user_id.server_name().as_str());
	fs::create_dir_all(&path);
	path.push("config");
	let client_config = ClientConfig::new().store_path(path);
	let client = Client::new_from_user_id_with_config(user_id.clone(), client_config).await.unwrap();
	client.login(user_id.localpart(), pass.trim_end(), None, None).await;

	client.register_event_handler(
            |ev: SyncMessageEvent<MessageEventContent>| async move {
                println!("{:?}", ev);
            },
        ).await;

	client.sync(SyncSettings::default()).await;
}
