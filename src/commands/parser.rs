//! nom v8 grammar for CAD command strings.
//!
//! Parses invocations like "LINE 0,0 100,100", "CIRCLE 50,50 25",
//! "ARC 0,0 50 0 90", and "PLINE 0,0 100,0 100,100 C".

use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::tag_no_case,
    character::complete::{char, space0, space1},
    combinator::opt,
    number::complete::float,
    sequence::preceded,
};
use crate::geometry::Point2D;

/// The result of parsing a command invocation.
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedCommand {
    Line(LineArgs),
    Circle(CircleArgs),
    Arc(ArcArgs),
    Polyline(PolylineArgs),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineArgs {
    pub start: Option<Point2D>,  // None if just "LINE" with no args
    pub end: Option<Point2D>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CircleArgs {
    pub center: Option<Point2D>,
    pub radius: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArcArgs {
    pub center: Option<Point2D>,
    pub radius: Option<f64>,
    pub start_angle: Option<f64>,
    pub end_angle: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolylineArgs {
    pub vertices: Vec<Point2D>,
    pub closed: bool,
}

/// Parse a 2D point from strings like:
/// - "10,20"
/// - "10.5, 20.3"
pub fn parse_point(input: &str) -> IResult<&str, Point2D> {
    (float, preceded(space0, char(',')), preceded(space0, float))
        .map(|(x, _, y)| Point2D::new(x as f64, y as f64))
        .parse(input)
}

/// Parse a command invocation:
/// - "LINE"
/// - "LINE 0,0 100,100"
/// - "L" (alias)
/// - "CIRCLE 50,50 25"
pub fn parse_command(input: &str) -> IResult<&str, ParsedCommand> {
    // Parse leading whitespace
    let (input, _) = space0.parse(input)?;

    // Match command name (case-insensitive)
    let (rest, cmd_token) = alt((
        tag_no_case("LINE"),
        tag_no_case("L"),
        tag_no_case("CIRCLE"),
        tag_no_case("C"),
        tag_no_case("ARC"),
        tag_no_case("A"),
        tag_no_case("PLINE"),
        tag_no_case("PL"),
    ))
    .parse(input)?;

    // If there is remaining content, whitespace between command name and
    // args is required. This prevents "LINE0,0" from being parsed as a
    // bare "LINE" command while still allowing bare "LINE" to succeed.
    let rest = if !rest.is_empty() {
        let (r, _) = space1.parse(rest)?;
        r
    } else {
        rest
    };

    match cmd_token.to_uppercase().as_str() {
        "LINE" | "L" => {
            // LINE [start_x,start_y] [end_x,end_y]
            let (rest, first) = opt(parse_point).parse(rest)?;
            let (rest, second) = if first.is_some() {
                let (rest, _) = space0.parse(rest)?;
                opt(parse_point).parse(rest)?
            } else {
                (rest, None)
            };
            Ok((rest, ParsedCommand::Line(LineArgs {
                start: first,
                end: second,
            })))
        }
        "CIRCLE" | "C" => {
            // CIRCLE [center_x,center_y] [radius]
            let (rest, center) = opt(parse_point).parse(rest)?;
            let (rest, radius) = if center.is_some() {
                let (rest, _) = space0.parse(rest)?;
                let (rest, r) = opt(float).parse(rest)?;
                (rest, r.map(|v| v as f64))
            } else {
                (rest, None)
            };
            Ok((rest, ParsedCommand::Circle(CircleArgs {
                center,
                radius,
            })))
        }
        "ARC" | "A" => {
            // ARC [center_x,center_y] [radius] [start_angle] [end_angle]
            // NOTE: Arc angles should be validated — if start_angle >= end_angle,
            // the ArcCommand should return an error or treat as a 360° sweep.
            let (rest, center) = opt(parse_point).parse(rest)?;
            let (rest, radius) = if center.is_some() {
                let (rest, _) = space0.parse(rest)?;
                let (rest, r) = opt(float).parse(rest)?;
                (rest, r.map(|v| v as f64))
            } else {
                (rest, None)
            };
            let (rest, start_angle) = if radius.is_some() {
                let (rest, _) = space0.parse(rest)?;
                let (rest, a) = opt(float).parse(rest)?;
                (rest, a.map(|v| v as f64))
            } else {
                (rest, None)
            };
            let (rest, end_angle) = if start_angle.is_some() {
                let (rest, _) = space0.parse(rest)?;
                let (rest, a) = opt(float).parse(rest)?;
                (rest, a.map(|v| v as f64))
            } else {
                (rest, None)
            };
            Ok((rest, ParsedCommand::Arc(ArcArgs {
                center,
                radius,
                start_angle,
                end_angle,
            })))
        }
        "PLINE" | "PL" => {
            // PLINE x1,y1 x2,y2 ... [C]
            // Collect points until no more valid points or C/Close token
            //
            // NOTE: Close token "C" must be followed by whitespace or
            // end-of-input to avoid false-matching coordinate text like
            // "C10,20" (which should parse as a vertex, not a close).
            let mut vertices = Vec::new();
            let mut rest = rest;

            loop {
                let (r, _) = space0::<&str, nom::error::Error<&str>>(rest).unwrap_or((rest, ""));

                // Check for close token followed by whitespace or EOF.
                // tag_no_case("C").then(space0) would succeed on "C10,20"
                // because space0 matches zero spaces, so we also check
                // that the next char (if any) is not alphanumeric.
                if let Ok((after_c, _)) = tag_no_case::<_, _, nom::error::Error<_>>("C").parse(r) {
                    let next_is_end_or_ws = after_c.is_empty()
                        || after_c.chars().next().map(|c| c.is_ascii_whitespace()).unwrap_or(false);
                    if next_is_end_or_ws {
                        return Ok((after_c, ParsedCommand::Polyline(PolylineArgs {
                            vertices,
                            closed: true,
                        })));
                    }
                }

                match parse_point(r) {
                    Ok((r, pt)) => {
                        vertices.push(pt);
                        rest = r;
                    }
                    Err(_) => break,
                }
            }

            if vertices.is_empty() {
                return Err(nom::Err::Error(nom::error::Error::new(
                    rest,
                    nom::error::ErrorKind::Fail,
                )));
            }

            Ok((rest, ParsedCommand::Polyline(PolylineArgs {
                vertices,
                closed: false,
            })))
        }
        _ => unreachable!(), // Guaranteed by alt() above
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_point() {
        let result = parse_point("10,20");
        assert_eq!(result, Ok(("", Point2D::new(10.0, 20.0))));
    }

    #[test]
    fn test_parse_point_with_spaces() {
        let result = parse_point("10.5, 20.25");
        assert_eq!(result, Ok(("", Point2D::new(10.5, 20.25))));
    }

    #[test]
    fn test_parse_line() {
        let result = parse_command("LINE 0,0 100,100");
        assert!(result.is_ok());
        let (remaining, cmd) = result.unwrap();
        assert!(remaining.is_empty());
        assert_eq!(
            cmd,
            ParsedCommand::Line(LineArgs {
                start: Some(Point2D::new(0.0, 0.0)),
                end: Some(Point2D::new(100.0, 100.0)),
            })
        );
    }

    #[test]
    fn test_parse_line_bare() {
        let result = parse_command("LINE");
        assert!(result.is_ok());
        let (remaining, cmd) = result.unwrap();
        assert!(remaining.is_empty());
        assert_eq!(
            cmd,
            ParsedCommand::Line(LineArgs {
                start: None,
                end: None,
            })
        );
    }

    #[test]
    fn test_parse_line_alias() {
        let result = parse_command("L 10,20 30,40");
        assert!(result.is_ok());
        let (remaining, cmd) = result.unwrap();
        assert!(remaining.is_empty());
        assert_eq!(
            cmd,
            ParsedCommand::Line(LineArgs {
                start: Some(Point2D::new(10.0, 20.0)),
                end: Some(Point2D::new(30.0, 40.0)),
            })
        );
    }

    #[test]
    fn test_parse_line_no_whitespace() {
        // Whitespace is required after command name, so "LINE0,0" should fail.
        assert!(parse_command("LINE0,0").is_err());
    }

    #[test]
    fn test_parse_circle() {
        let result = parse_command("CIRCLE 50,50 25");
        assert!(result.is_ok());
        let (remaining, cmd) = result.unwrap();
        assert!(remaining.is_empty());
        assert_eq!(
            cmd,
            ParsedCommand::Circle(CircleArgs {
                center: Some(Point2D::new(50.0, 50.0)),
                radius: Some(25.0),
            })
        );
    }

    #[test]
    fn test_parse_circle_bare() {
        let result = parse_command("CIRCLE");
        assert!(result.is_ok());
        let (remaining, cmd) = result.unwrap();
        assert!(remaining.is_empty());
        assert_eq!(
            cmd,
            ParsedCommand::Circle(CircleArgs {
                center: None,
                radius: None,
            })
        );
    }

    #[test]
    fn test_parse_circle_alias() {
        let result = parse_command("C 10,20 5.5");
        assert!(result.is_ok());
        let (remaining, cmd) = result.unwrap();
        assert!(remaining.is_empty());
        assert_eq!(
            cmd,
            ParsedCommand::Circle(CircleArgs {
                center: Some(Point2D::new(10.0, 20.0)),
                radius: Some(5.5),
            })
        );
    }

    #[test]
    fn test_parse_arc() {
        let result = parse_command("ARC 0,0 50 0 90");
        assert!(result.is_ok());
        let (remaining, cmd) = result.unwrap();
        assert!(remaining.is_empty());
        assert_eq!(
            cmd,
            ParsedCommand::Arc(ArcArgs {
                center: Some(Point2D::new(0.0, 0.0)),
                radius: Some(50.0),
                start_angle: Some(0.0),
                end_angle: Some(90.0),
            })
        );
    }

    #[test]
    fn test_parse_arc_alias() {
        let result = parse_command("A 1,2 10 45 180");
        assert!(result.is_ok());
        let (remaining, cmd) = result.unwrap();
        assert!(remaining.is_empty());
        assert_eq!(
            cmd,
            ParsedCommand::Arc(ArcArgs {
                center: Some(Point2D::new(1.0, 2.0)),
                radius: Some(10.0),
                start_angle: Some(45.0),
                end_angle: Some(180.0),
            })
        );
    }

    #[test]
    fn test_parse_pline_closed() {
        let result = parse_command("PLINE 0,0 100,0 100,100 0,100 C");
        assert!(result.is_ok());
        let (remaining, cmd) = result.unwrap();
        assert!(remaining.is_empty());
        assert_eq!(
            cmd,
            ParsedCommand::Polyline(PolylineArgs {
                vertices: vec![
                    Point2D::new(0.0, 0.0),
                    Point2D::new(100.0, 0.0),
                    Point2D::new(100.0, 100.0),
                    Point2D::new(0.0, 100.0),
                ],
                closed: true,
            })
        );
    }

    #[test]
    fn test_parse_pline_open() {
        let result = parse_command("PLINE 0,0 100,0 100,100");
        assert!(result.is_ok());
        let (remaining, cmd) = result.unwrap();
        assert!(remaining.is_empty());
        assert_eq!(
            cmd,
            ParsedCommand::Polyline(PolylineArgs {
                vertices: vec![
                    Point2D::new(0.0, 0.0),
                    Point2D::new(100.0, 0.0),
                    Point2D::new(100.0, 100.0),
                ],
                closed: false,
            })
        );
    }

    #[test]
    fn test_parse_pline_alias() {
        let result = parse_command("PL 0,0 10,0 C");
        assert!(result.is_ok());
        let (remaining, cmd) = result.unwrap();
        assert!(remaining.is_empty());
        assert_eq!(
            cmd,
            ParsedCommand::Polyline(PolylineArgs {
                vertices: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
                closed: true,
            })
        );
    }

    #[test]
    fn test_parse_invalid_command() {
        assert!(parse_command("UNKNOWN").is_err());
    }

    #[test]
    fn test_parse_line_partial() {
        // "LINE 0,0" supplies a start point but no end point.
        // The parser consumes the point; the remaining is empty because
        // "0,0" was fully parsed as a point. The command system would
        // then interactively prompt for the end point.
        let result = parse_command("LINE 0,0");
        assert!(result.is_ok());
        let (remaining, cmd) = result.unwrap();
        assert!(remaining.is_empty());
        assert_eq!(
            cmd,
            ParsedCommand::Line(LineArgs {
                start: Some(Point2D::new(0.0, 0.0)),
                end: None,
            })
        );
    }
}
