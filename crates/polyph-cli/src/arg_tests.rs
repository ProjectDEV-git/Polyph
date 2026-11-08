use super::*;

fn parse(args: &[&str]) -> Cmd {
    let mut v = vec!["polyph"];
    v.extend_from_slice(args);
    Cli::try_parse_from(v).unwrap().cmd.unwrap()
}

#[test]
fn start_room_and_hidden_name_set_the_room() {
    for flag in ["--room", "--name"] {
        let Cmd::Start { room, me, yes, .. } = parse(&["start", flag, "r1", "--yes"]) else { panic!() };
        assert_eq!(room.as_deref(), Some("r1"));
        assert_eq!(me, None);
        assert!(yes);
    }
}

#[test]
fn start_me_sets_the_person() {
    let Cmd::Start { room, me, .. } = parse(&["start", "--me", "Ann"]) else { panic!() };
    assert_eq!(me.as_deref(), Some("Ann"));
    assert_eq!(room, None);
}

#[test]
fn join_me_and_hidden_name_set_the_person() {
    for flag in ["--me", "--name"] {
        let Cmd::Join { code, me, .. } = parse(&["join", "A1B2-CDEF", flag, "Bo"]) else { panic!() };
        assert_eq!(code, "A1B2-CDEF");
        assert_eq!(me.as_deref(), Some("Bo"));
    }
}
