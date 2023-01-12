use matrix_sdk::{
    room::Room,
    ruma::{
        api::client::r0::{
            filter::{FilterDefinition, LazyLoadOptions, RoomEventFilter},
            message::get_message_events::Request,
            sync::sync_events::Filter,
        },
        events::{
            room::message::{
                ImageMessageEventContent, MessageEventContent, MessageType, TextMessageEventContent,
            },
            AnyMessageEvent::RoomMessage,
            AnyMessageEventContent, AnyRoomEvent, SyncMessageEvent,
        },
        UserId,
    },
    Client, SyncSettings,
};
use std::{collections::HashMap, env, io, io::BufRead, io::Write};
use termios::*;

use matrix_sdk_base::RoomType;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        panic!("usage: mmm user");
    }

    let user_id = match UserId::try_from(args[1].clone()) {
        Ok(user_id) => user_id,
        Err(e) => panic!("Bad user_id: {e}"),
    };

    let client = Client::new_from_user_id(user_id.clone()).await.unwrap();

    /* disable echo */
    let mut termios = Termios::from_fd(0).unwrap();
    tcgetattr(0, &mut termios);
    termios.c_lflag &= !ECHO;
    tcsetattr(0, TCSANOW, &termios);

    /* TODO: zeroize password after login */
    while !client.logged_in().await {
        print!("Password: ");
        io::stdout().flush();
        let pass = io::stdin().lock().lines().next().unwrap().unwrap();
        println!("");
        client.login(user_id.localpart(), &pass, None, None).await;
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

    /* cache joined rooms */
    let mut name_to_room = HashMap::new();
    for room in client.joined_rooms() {
        if let Ok(name) = room.display_name().await {
            name_to_room.insert(name, Room::Joined(room.clone()));
        }
    }

    let sync_token = client.sync_token().await.unwrap();
    let settings = SyncSettings::default().token(sync_token.clone());
    tokio::spawn(async move { shell(name_to_room, &sync_token).await });
    client.sync(settings).await;
}

async fn shell(name_to_room: HashMap<String, Room>, sync_token: &str) {
    let mut name = String::new();
    loop {
        let mut send = true;
        print!("{name}> ");
        io::stdout().flush();
        let cmd = io::stdin().lock().lines().next().unwrap().unwrap();
        if cmd.chars().next().unwrap() == '/' {
            name = cmd[1..].to_string().clone();
            send = false;
        }

        if let Some(Room::Joined(room)) = name_to_room.get(&name) {
            if send {
                let content =
                    AnyMessageEventContent::RoomMessage(MessageEventContent::text_plain(cmd));
                room.send(content, None).await.unwrap();
            } else {
                let request = Request::backward(&room.room_id(), sync_token);
                for chunk in room.messages(request).await.unwrap().chunk {
                    if let AnyRoomEvent::Message(msg) = chunk.deserialize().unwrap() {
                        let time = msg.origin_server_ts().get();
                        if let RoomMessage(event) = msg.clone() {
                            let body = parse_message_event_content(&event.content);
                            println!("{:?} {name} {} {body}", time, msg.sender());
                        };
                    };
                }
            }
        };
    }
}

fn parse_message_event_content(content: &MessageEventContent) -> String {
    /* TODO: image, video, file, audio */
    return match content.msgtype.clone() {
        MessageType::Text(TextMessageEventContent { body: _body, .. }) => _body,
        MessageType::Image(ImageMessageEventContent {
            body: _body,
            url: Some(_url),
            ..
        }) => {
            format!("{_body}({_url})")
        }
        _ => String::new(),
    };
}

async fn on_room_msg(ev: SyncMessageEvent<MessageEventContent>, room: Room) {
    if room.room_type() != RoomType::Joined {
        return;
    }

    let body = parse_message_event_content(&ev.content);

    println!(
        "{} {} {} {body}",
        ev.origin_server_ts.get(),
        room.display_name().await.unwrap(),
        ev.sender.localpart()
    );
}
