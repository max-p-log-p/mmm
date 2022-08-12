use std::{collections::HashMap, env, fs, io, io::Write, path::PathBuf, thread};
use termios::*;
use matrix_sdk::{
	Client, ClientConfig,
	room::Room,
	ruma::{
		api::client::r0::{
			filter::{
			    FilterDefinition, LazyLoadOptions, 
			    RoomEventFilter,
			},
			sync::sync_events::Filter,
		},
		events::{
			AnyMessageEventContent,
			SyncMessageEvent, 
			room::message::{
				MessageEventContent, MessageType, 
				TextMessageEventContent, ImageMessageEventContent,
			},
		},
		RoomId, UserId
	},
	SyncSettings,
};

use matrix_sdk_base::{
	RoomType,
};

#[tokio::main]
async fn main() {
	let args: Vec<String> = env::args().collect(); 

	if args.len() < 2 {
		panic!("usage: mmm user [prefix]");
	}

	let user_id = match UserId::try_from(args[1].clone()) { 
		Ok(user_id) => user_id,
		Err(e) => panic!("Bad user_id: {e}"),
	};

	/* configure client */
	let mut path: PathBuf = if args.len() >= 3 {
		PathBuf::from(args[2].clone())
	} else {
		env::home_dir().expect("no home directory")
	};
	path.push("mmm/");
	fs::create_dir_all(&path);
	path.push("config");
	let client_config = ClientConfig::new().store_path(&path);
	let client = Client::new_from_user_id_with_config(user_id.clone(), client_config).await.unwrap();

	/* disable echo */
	let mut termios = Termios::from_fd(0).unwrap();
	tcgetattr(0, &mut termios);
	termios.c_lflag &= !ECHO;
	tcsetattr(0, TCSANOW, &termios);

	/* TODO: zeroize password after login */
	while !client.logged_in().await {
		let mut pass = String::new();
		print!("Password: ");
		io::stdout().flush();
		io::stdin().read_line(&mut pass);
		println!("");
		client.login(user_id.localpart(), pass.trim_end(), None, None).await;
	}

	/* enable echo */
	termios.c_lflag |= ECHO;
	tcsetattr(0, TCSANOW, &termios);

	/* enable lazy loading to fix room names */
        let mut filter = FilterDefinition::default();
        let mut event_filter = RoomEventFilter::default();
        event_filter.lazy_load_options = LazyLoadOptions::Enabled {
		include_redundant_members: true,
	};
        filter.room.state = event_filter;
        let sync_settings = SyncSettings::new().filter(Filter::FilterDefinition(filter));

	client.register_event_handler(on_room_msg).await;
	client.sync_once(sync_settings).await.unwrap();

	let mut name_to_room = HashMap::new();

	for room in client.joined_rooms() {
		if let Ok(name) = room.display_name().await {
			name_to_room.insert(name, Room::Joined(room.clone()));
		}
	}
	
	println!("Spawning thread");
	tokio::spawn(async { read_and_send(name_to_room).await });
	println!("Thread spawned");

	let settings = SyncSettings::default().token(client.sync_token().await.unwrap());
	client.sync(settings).await;
}

async fn read_and_send(name_to_room: HashMap<String, Room>) {
	loop {
		print!("> "); 
		io::stdout().flush();
		let mut line = String::new();
		io::stdin().read_line(&mut line);
		let mut split = line.splitn(2, "|");
		if let Some(Room::Joined(room)) = name_to_room.get(split.next().unwrap()) {
			let content = AnyMessageEventContent::RoomMessage(MessageEventContent::text_plain(split.next().unwrap()));
			room.send(content, None).await.unwrap();
		}
	}
}

async fn on_room_msg(ev: SyncMessageEvent<MessageEventContent>, room: Room) {
	if room.room_type() != RoomType::Joined {
		return;
	}

	/* TODO: image, video, file, audio */
	let content = match ev.content.msgtype {
		MessageType::Text(TextMessageEventContent { body: _body, .. }) => _body,
		MessageType::Image(ImageMessageEventContent { body: _body, url: Some(_url), .. }) => {
			format!("{_body}({_url})")
		},
		_ => return,
	};

	println!("{} {} {} {content}", ev.origin_server_ts.get(), room.display_name().await.unwrap(), ev.sender.localpart());
}
