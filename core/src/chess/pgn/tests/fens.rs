//! The contract of [`Pgn::fens`]: the positions a game's [mainline](Pgn) passes through.
//! Moves are played by the rules of standard chess from the [`[FEN]` tag](Pgn), or from the
//! starting position without one. The [en passant square](Fen) follows the [PGN](Pgn) spec: it's
//! set after every double pawn push. The first [ply](crate::Ply) that can't be played is an error
//! that names it, and nothing follows it.

use crate::chess::{Fen, Pgn};

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Everything `pgn.fens()` yields, as text, up to a bound in case it never stops.
fn items(pgn: &Pgn) -> Vec<Result<String, String>> {
    pgn.fens()
        .take(10_000)
        .map(|item| {
            item.map(|fen| fen.to_string())
                .map_err(|err| err.to_string())
        })
        .collect()
}

#[track_caller]
fn parse(pgn: &str) -> Pgn {
    pgn.parse().unwrap_or_else(|err| panic!("{pgn:?}: {err}"))
}

/// A game that starts from `fen`.
fn from(fen: &str, movetext: &str) -> String {
    format!("[FEN \"{fen}\"]\n\n{movetext}")
}

/// `pgn` yields `expected`, starting position included, and no error.
#[track_caller]
fn check(pgn: &str, expected: &[&str]) {
    let items = items(&parse(pgn));
    let expected: Vec<Result<String, String>> =
        expected.iter().map(|fen| Ok(fen.to_string())).collect();
    assert_eq!(items, expected, "{pgn:?}");
}

/// `pgn` yields no error, and `expected` last.
#[track_caller]
fn check_last(pgn: &str, expected: &str) {
    let items = items(&parse(pgn));
    assert!(items.iter().all(Result::is_ok), "{pgn:?}: {items:?}");
    assert_eq!(items.last(), Some(&Ok(expected.to_string())), "{pgn:?}");
}

/// `pgn` yields `ok` positions, then an error that contains `expected`, then nothing.
#[track_caller]
fn check_err(pgn: &str, ok: usize, expected: &str) {
    let items = items(&parse(pgn));
    assert_eq!(items.len(), ok + 1, "{pgn:?}: {items:?}");
    assert!(items[..ok].iter().all(Result::is_ok), "{pgn:?}: {items:?}");
    let err = items[ok].as_ref().expect_err(pgn);
    assert!(err.contains(expected), "{pgn:?}: {err}");
}

mod spec {
    use super::{START, check, check_err, check_last, from};

    #[test]
    fn a_game_without_moves_is_its_start() {
        check("", &[START]);
        check("[Event \"x\"] *", &[START]);
    }

    #[test]
    fn moves_in_order() {
        check(
            "1. e4 e5 2. Nf3",
            &[
                START,
                "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
                "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq e6 0 2",
                "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2",
            ],
        );
    }

    #[test]
    fn counters() {
        check_last(
            "1. Nf3 Nf6 2. Ng1 Ng8",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 4 3",
        );
        // A pawn move resets the halfmove clock.
        check_last(
            "1. Nf3 Nf6 2. e3",
            "rnbqkb1r/pppppppp/5n2/8/8/4PN2/PPPP1PPP/RNBQKB1R b KQkq - 0 2",
        );
        // So does a capture.
        check_last(
            "1. e4 d5 2. exd5 Qxd5",
            "rnb1kbnr/ppp1pppp/8/3q4/8/8/PPPP1PPP/RNBQKBNR w KQkq - 0 3",
        );
        check_last(
            "1. e4 d5 2. exd5 Qxd5 3. Nc3 Qa5",
            "rnb1kbnr/ppp1pppp/8/q7/8/2N5/PPPP1PPP/R1BQKBNR w KQkq - 2 4",
        );
        // The tag's counters carry on.
        check_last(
            &from("4k3/8/8/8/8/8/8/4K3 w - - 10 30", "30. Kf1 Kf8 31. Ke1"),
            "5k2/8/8/8/8/8/8/4K3 b - - 13 31",
        );
    }

    #[test]
    fn en_passant() {
        check(
            "1. e4 Nf6 2. e5 d5 3. exd6",
            &[
                START,
                "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
                "rnbqkb1r/pppppppp/5n2/8/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 1 2",
                "rnbqkb1r/pppppppp/5n2/4P3/8/8/PPPP1PPP/RNBQKBNR b KQkq - 0 2",
                "rnbqkb1r/ppp1pppp/5n2/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3",
                "rnbqkb1r/ppp1pppp/3P1n2/8/8/8/PPPP1PPP/RNBQKBNR b KQkq - 0 3",
            ],
        );
        check_last(
            "1. Nf3 d5 2. Nc3 d4 3. e4 dxe3",
            "rnbqkbnr/ppp1pppp/8/8/8/2N1pN2/PPPP1PPP/R1BQKB1R w KQkq - 0 4",
        );
        // The square is set even when no pawn can capture.
        check_last(
            "1. a4",
            "rnbqkbnr/pppppppp/8/8/P7/8/1PPPPPPP/RNBQKBNR b KQkq a3 0 1",
        );
    }

    #[test]
    fn promotions() {
        for (promotion, piece) in [("Q", 'Q'), ("R", 'R'), ("B", 'B'), ("N", 'N')] {
            check_last(
                &from(
                    "7k/P7/8/8/8/8/8/K7 w - - 3 1",
                    &format!("1. a8={promotion}"),
                ),
                &format!("{piece}6k/8/8/8/8/8/8/K7 b - - 0 1"),
            );
        }
        check_last(
            &from("1n5k/P7/8/8/8/8/8/K7 w - - 3 1", "1. axb8=N"),
            "1N5k/8/8/8/8/8/8/K7 b - - 0 1",
        );
        check_last(
            &from("k7/8/8/8/8/8/7p/K7 b - - 3 1", "1... h1=Q"),
            "k7/8/8/8/8/8/8/K6q w - - 0 2",
        );
    }

    #[test]
    fn castling() {
        let start = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1";
        for (movetext, expected) in [
            ("1. O-O", "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R4RK1 b kq - 1 1"),
            (
                "1. O-O-O",
                "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/2KR3R b kq - 1 1",
            ),
            (
                "1. O-O O-O-O",
                "2kr3r/pppppppp/8/8/8/8/PPPPPPPP/R4RK1 w - - 2 2",
            ),
            (
                "1. 0-0-0 0-0",
                "r4rk1/pppppppp/8/8/8/8/PPPPPPPP/2KR3R w - - 2 2",
            ),
        ] {
            check_last(&from(start, movetext), expected);
        }
        // Only the king's path must be safe: the rook may pass an attacked square.
        check_last(
            &from("r3k2r/8/8/4B3/8/8/8/R3K2R b KQkq - 0 1", "1... O-O-O"),
            "2kr3r/8/8/4B3/8/8/8/R3K2R w KQ - 1 2",
        );
        check_last(
            &from("r3k2r/8/8/8/2b5/8/8/R3K2R w KQkq - 0 1", "1. O-O-O"),
            "r3k2r/8/8/8/2b5/8/8/2KR3R b kq - 1 1",
        );
    }

    #[test]
    fn castling_rights_are_lost() {
        let start = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1";
        for (movetext, expected) in [
            // By moving the king, even back home.
            ("1. Kf1", "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R4K1R b kq - 1 1"),
            (
                "1. Kf1 Kf8 2. Ke1 Ke8",
                "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w - - 4 3",
            ),
            // By moving a rook.
            (
                "1. Rb1",
                "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/1R2K2R b Kkq - 1 1",
            ),
            (
                "1. Rf1 Rd8",
                "3rk2r/pppppppp/8/8/8/8/PPPPPPPP/R3KR2 w Qk - 2 2",
            ),
        ] {
            check_last(&from(start, movetext), expected);
        }
        // By losing a rook on its home square.
        check_last(
            &from("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", "1. Rxa8+"),
            "R3k2r/8/8/8/8/8/8/4K2R b Kk - 0 1",
        );
        check_last(
            &from("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", "1. Rxh8+"),
            "r3k2R/8/8/8/8/8/8/R3K3 b Qq - 0 1",
        );
        check_last(
            &from("k7/8/8/8/8/8/1p6/R3K3 b Q - 0 1", "1... bxa1=R+"),
            "k7/8/8/8/8/8/8/r3K3 w - - 0 2",
        );
    }

    #[test]
    fn disambiguation() {
        let knights = "4k3/8/8/8/8/8/8/1N2KN2 w - - 0 1";
        check_last(
            &from(knights, "1. Nbd2"),
            "4k3/8/8/8/8/8/3N4/4KN2 b - - 1 1",
        );
        check_last(
            &from(knights, "1. Nfd2"),
            "4k3/8/8/8/8/8/3N4/1N2K3 b - - 1 1",
        );
        let rooks = "4k3/8/8/R7/8/8/8/R3K3 w - - 0 1";
        check_last(&from(rooks, "1. R1a3"), "4k3/8/8/R7/8/R7/8/4K3 b - - 1 1");
        check_last(&from(rooks, "1. R5a3"), "4k3/8/8/8/8/R7/8/R3K3 b - - 1 1");
        // Qa1, Qa3 and Qc1 all reach b2.
        let queens = "4k3/8/8/8/8/Q7/8/Q1Q1K3 w - - 0 1";
        check_last(
            &from(queens, "1. Qa1b2"),
            "4k3/8/8/8/8/Q7/1Q6/2Q1K3 b - - 1 1",
        );
        check_last(
            &from(queens, "1. Qcb2"),
            "4k3/8/8/8/8/Q7/1Q6/Q3K3 b - - 1 1",
        );
        check_last(
            &from(queens, "1. Q3b2"),
            "4k3/8/8/8/8/8/1Q6/Q1Q1K3 b - - 1 1",
        );
    }

    #[test]
    fn a_pinned_piece_needs_no_disambiguation() {
        // The knight on e2 is pinned, so only the one on b1 can go to c3.
        check_last(
            &from("4k3/4r3/8/8/8/8/4N3/1N2K3 w - - 0 1", "1. Nc3"),
            "4k3/4r3/8/8/8/2N5/4N3/4K3 b - - 1 1",
        );
    }

    #[test]
    fn needless_disambiguation_is_accepted() {
        let expected = "rnbqkbnr/pppppppp/8/8/8/5N2/PPPPPPPP/RNBQKB1R b KQkq - 1 1";
        for movetext in ["1. Ngf3", "1. N1f3", "1. Ng1f3"] {
            check_last(movetext, expected);
        }
    }

    #[test]
    fn check_and_mate_suffixes_are_not_checked() {
        let expected = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq e6 0 2";
        for movetext in ["1. e4 e5", "1. e4+ e5#", "1. e4# e5+"] {
            check_last(movetext, expected);
        }
        check_last(
            "1. f3 e5 2. g4 Qh4",
            "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
        );
    }

    #[test]
    fn null_moves_pass_the_turn() {
        check(
            "1. e4 -- 2. d4",
            &[
                START,
                "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
                "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 1 2",
                "rnbqkbnr/pppppppp/8/8/3PP3/8/PPP2PPP/RNBQKBNR b KQkq d3 0 2",
            ],
        );
    }

    #[test]
    fn the_fen_tag_sets_the_start() {
        let after_e4 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
        let after_e5 = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq e6 0 2";
        for pgn in [
            format!("[SetUp \"1\"]\n[FEN \"{after_e4}\"]\n\n1... e5"),
            format!("[FEN \"{after_e4}\"]\n\n1... e5"),
            format!("[Variant \"From Position\"]\n[FEN \"{after_e4}\"]\n\n1... e5"),
        ] {
            check(&pgn, &[after_e4, after_e5]);
        }
        check(&from(START, ""), &[START]);
    }

    #[test]
    fn standard_is_the_only_variant() {
        check("[Variant \"Standard\"]", &[START]);
        check("[Variant \"From Position\"]", &[START]);
        for variant in ["Chess960", "Crazyhouse", "Three-check", "Atomic"] {
            check_err(
                &format!("[Variant \"{variant}\"]\n\n1. e4"),
                0,
                &format!("unsupported variant {variant}"),
            );
        }
        // The variant is checked before the FEN tag, which may be the variant's.
        check_err(
            "[Variant \"Chess960\"]\n[FEN \"qbrnkrbn/pppppppp/8/8/8/8/PPPPPPPP/QBRNKRBN w KQkq - 0 1\"]",
            0,
            "unsupported variant Chess960",
        );
    }

    #[test]
    fn comments_variations_and_annotations_change_nothing() {
        let expected = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq e6 0 2";
        for movetext in [
            "1. e4 { 1. d4 } (1. d4 d5) 1... e5 $1 *",
            "1. e4! e5?! ; 2. Nf3\n",
            "1.e4 (1. Nz9 hello) e5 1-0 { after }",
        ] {
            check_last(movetext, expected);
        }
    }

    #[test]
    fn errors_name_the_move() {
        check_err("1. e5", 1, "1. e5");
        check_err("1. e4 e4", 2, "1... e4");
        check_err(
            &from("4k3/8/8/8/8/8/8/4K3 w - - 0 30", "30. Kd1 Kd8 31. Kd3"),
            3,
            "31. Kd3",
        );
    }

    #[test]
    fn no_piece_can_make_the_move() {
        for (movetext, ok, expected) in [
            ("1. e5", 1, "1. e5"),
            ("1. Nf4", 1, "1. Nf4"),
            ("1. Qd3", 1, "1. Qd3"),
            ("1. Bc4", 1, "1. Bc4"),
            ("1. Ke2", 1, "1. Ke2"),
            ("1. Rh3", 1, "1. Rh3"),
            ("1. e4 e5 2. e5", 3, "2. e5"),
            ("1. Nd2", 1, "1. Nd2"),
            ("1. e4 d5 2. Nxd5", 3, "2. Nxd5"),
        ] {
            check_err(movetext, ok, expected);
        }
    }

    #[test]
    fn captures_need_something_to_capture() {
        for (movetext, ok, expected) in [
            ("1. e4 e5 2. exd5", 3, "2. exd5"),
            ("1. e4 d5 2. exd6", 3, "2. exd6"),
            // En passant only right after the double push.
            ("1. e4 Nf6 2. e5 d5 3. Nf3 Nc6 4. exd6", 7, "4. exd6"),
            ("1. Nxf3", 1, "1. Nxf3"),
            // A pawn's own piece.
            ("1. e3 d6 2. Nc3 Bf5 3. dxc3", 5, "3. dxc3"),
            // A piece's own piece.
            ("1. Nxd2", 1, "1. Nxd2"),
        ] {
            check_err(movetext, ok, expected);
        }
    }

    #[test]
    fn a_capture_needs_its_x() {
        check_err("1. e4 d5 2. Nc3 d4 3. Nd5 Qd5", 6, "3... Qd5");
        check_err("1. e4 d5 2. ed5", 3, "ed5");
    }

    #[test]
    fn the_king_may_not_be_left_in_check() {
        for (pgn, ok, expected) in [
            // Moving into check.
            ("1. e4 e5 2. Ke2 Bc5 3. Ke3", 5, "3. Ke3"),
            // Not getting out of it.
            ("1. e4 e5 2. f4 Qh4+ 3. a3", 5, "3. a3"),
            // Moving a pinned piece.
            (
                &from("4k3/4r3/8/8/8/8/4N3/4K3 w - - 0 1", "1. Nc3"),
                1,
                "1. Nc3",
            ),
            (
                &from("4k3/4r3/8/8/8/8/4N3/1N2K3 w - - 0 1", "1. Nec3"),
                1,
                "1. Nec3",
            ),
            // Capturing en passant while pinned along the rank.
            (
                &from("8/8/8/K2pP2r/8/8/8/7k w - d6 0 1", "1. exd6"),
                1,
                "1. exd6",
            ),
            // The king capturing a defended piece.
            ("1. e4 f5 2. Ke2 fxe4 3. Ke3 d5 4. Kxe4", 7, "4. Kxe4"),
        ] {
            check_err(pgn, ok, expected);
        }
    }

    #[test]
    fn castling_needs_rights_room_and_safety() {
        for (fen, movetext, expected) in [
            // No right.
            ("r3k2r/8/8/8/8/8/8/R3K2R w Qkq - 0 1", "1. O-O", "1. O-O"),
            (
                "r3k2r/8/8/8/8/8/8/R3K2R b KQk - 0 1",
                "1... O-O-O",
                "1... O-O-O",
            ),
            // A piece in the way.
            (
                "r3k2r/8/8/8/8/8/8/RN2K2R w KQkq - 0 1",
                "1. O-O-O",
                "1. O-O-O",
            ),
            ("r3k2r/8/8/8/8/8/8/R3KB1R w KQkq - 0 1", "1. O-O", "1. O-O"),
            // Out of check.
            ("4k3/4r3/8/8/8/8/8/R3K2R w KQ - 0 1", "1. O-O", "1. O-O"),
            // Through an attacked square.
            ("r3k2r/8/8/8/2b5/8/8/R3K2R w KQkq - 0 1", "1. O-O", "1. O-O"),
            (
                "3rk2r/8/8/8/8/8/8/R3K2R w KQk - 0 1",
                "1. O-O-O",
                "1. O-O-O",
            ),
            // Into check.
            ("r3k2r/8/8/2b5/8/8/8/R3K2R w KQkq - 0 1", "1. O-O", "1. O-O"),
        ] {
            check_err(&from(fen, movetext), 1, expected);
        }
        check_err("1. O-O", 1, "1. O-O");
    }

    #[test]
    fn ambiguous_moves() {
        for (fen, movetext) in [
            ("4k3/8/8/8/8/8/8/1N2KN2 w - - 0 1", "1. Nd2"),
            ("4k3/8/8/R7/8/8/8/R3K3 w - - 0 1", "1. Ra3"),
            ("4k3/8/8/8/8/Q7/8/Q1Q1K3 w - - 0 1", "1. Qab2"),
            ("4k3/8/8/8/8/Q7/8/Q1Q1K3 w - - 0 1", "1. Q1b2"),
            ("4k3/8/8/8/8/Q7/8/Q1Q1K3 w - - 0 1", "1. Qb2"),
        ] {
            check_err(&from(fen, movetext), 1, movetext);
        }
    }

    #[test]
    fn variant_moves_are_not_standard() {
        check_err("1. N@f3", 1, "1. N@f3");
        check_err("1. e4 P@e6", 2, "1... P@e6");
        check_err(
            &from("7k/P7/8/8/8/8/8/K7 w - - 0 1", "1. a8=K"),
            1,
            "1. a8=K",
        );
    }

    #[test]
    fn a_word_that_is_not_san_is_an_error() {
        check_err("1. e4 Nz9", 2, "is not a SAN move");
        check_err("1. e4 e5 2. e8", 3, "is not a SAN move");
    }

    #[test]
    fn a_malformed_fen_tag_is_an_error() {
        for fen in [
            "",
            "start",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -",
        ] {
            check_err(&from(fen, "1. e4"), 0, &format!("{fen:?}"));
        }
    }

    #[test]
    fn a_fen_tag_that_cannot_be_played_is_an_error() {
        for fen in [
            // Kings.
            "4k3/8/8/8/8/8/8/8 w - - 0 1",
            "8/8/8/8/8/8/8/4K3 w - - 0 1",
            "4k3/8/8/8/8/8/8/3KK3 w - - 0 1",
            // Pawns on the first or last rank.
            "P3k3/8/8/8/8/8/8/4K3 w - - 0 1",
            "4k3/8/8/8/8/8/8/p3K3 w - - 0 1",
            // The side that isn't to move is in check.
            "4k3/8/8/8/8/8/8/4R1K1 w - - 0 1",
            // Castling rights without the king and rook at home.
            "4k3/8/8/8/8/8/8/4K3 w K - 0 1",
            "4k3/8/8/8/8/8/8/3K3R w K - 0 1",
            "4k3/8/8/8/8/8/8/R3K3 w KQ - 0 1",
            "4k3/8/8/8/8/8/8/4K3 w q - 0 1",
            // An en passant square without the pawn that just passed it.
            "4k3/8/8/8/8/8/8/4K3 b - e3 0 1",
            "4k3/8/8/8/4P3/8/4P3/4K3 b - e3 0 1",
            "4k3/8/8/8/4P3/4p3/8/4K3 b - e3 0 1",
        ] {
            check_err(&from(fen, "*"), 0, fen);
        }
    }
}

mod properties {
    use proptest::prelude::*;
    use proptest::test_runner::FileFailurePersistence;

    use super::*;
    use crate::chess::pgn::tests::games::{SOUP, game};

    proptest! {
        #![proptest_config(ProptestConfig {
            failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
            ..ProptestConfig::default()
        })]

        /// Positions, then at most one error, and nothing after it. Every position is a
        /// [`Fen`], and a game without an error has one per [ply](crate::Ply) and one more.
        #[test]
        fn any_game_yields_positions_then_at_most_one_error(
            text in prop_oneof![game().prop_map(|game| game.text()), SOUP, any::<String>()]
        ) {
            let Ok(pgn) = text.parse::<Pgn>() else { return Ok(()) };
            let items = items(&pgn);
            let ok = items.iter().take_while(|item| item.is_ok()).count();
            prop_assert!(items.len() <= ok + 1, "{:?}: {:?}", text, items);
            let fens: Vec<String> = items[..ok].iter().cloned().map(Result::unwrap).collect();
            for fen in &fens {
                let parsed = fen.parse::<Fen>().map_err(|err| TestCaseError::fail(format!("{fen:?}: {err}")))?;
                prop_assert_eq!(parsed.as_str(), fen);
            }
            if items.len() == ok {
                let plies = pgn.plies().map_err(|err| TestCaseError::fail(err.to_string()))?;
                prop_assert_eq!(ok, plies.len() + 1, "{:?}", text);
            }
        }

    }
}
