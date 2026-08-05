use crate::protocol::{DslDiagnostic, GateKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DslCommand {
    Help,
    Status,
    ActivateLogicCore,
    Create {
        kind: GateKind,
        id: String,
    },
    Remove {
        id: String,
    },
    Wire {
        source: String,
        target: String,
        port: String,
    },
    Unwire {
        target: String,
        port: String,
    },
    Set {
        sensor: String,
        value: bool,
    },
    Inspect {
        id: Option<String>,
    },
    ClearCircuit,
}

pub fn parse(input: &str) -> Result<DslCommand, DslDiagnostic> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(diagnostic(
            "empty_command",
            "enter a command",
            0,
            input.len(),
        ));
    }
    let tokens: Vec<_> = trimmed.split_whitespace().collect();
    let keyword = tokens[0].to_ascii_uppercase();
    match keyword.as_str() {
        "HELP" => exact(tokens.len(), 1, input).map(|()| DslCommand::Help),
        "STATUS" => exact(tokens.len(), 1, input).map(|()| DslCommand::Status),
        "ACTIVATE" if tokens.len() == 2 && tokens[1].eq_ignore_ascii_case("LOGIC_CORE") => {
            Ok(DslCommand::ActivateLogicCore)
        }
        "ACTIVATE" => Err(usage(input, "ACTIVATE LOGIC_CORE")),
        "CREATE" if tokens.len() == 3 => {
            let kind = parse_gate(tokens[1], input)?;
            Ok(DslCommand::Create {
                kind,
                id: normalize_id(tokens[2], input)?,
            })
        }
        "CREATE" => Err(usage(input, "CREATE <AND|OR|NOT|XOR> <ID>")),
        "REMOVE" if tokens.len() == 2 => Ok(DslCommand::Remove {
            id: normalize_id(tokens[1], input)?,
        }),
        "REMOVE" => Err(usage(input, "REMOVE <ID>")),
        "WIRE" if tokens.len() == 4 && tokens[2] == "->" => {
            let (target, port) = parse_target(tokens[3], input)?;
            Ok(DslCommand::Wire {
                source: normalize_id(tokens[1], input)?,
                target,
                port,
            })
        }
        "WIRE" => Err(usage(input, "WIRE <SOURCE> -> <TARGET>.<PORT>")),
        "UNWIRE" if tokens.len() == 2 => {
            let (target, port) = parse_target(tokens[1], input)?;
            Ok(DslCommand::Unwire { target, port })
        }
        "UNWIRE" => Err(usage(input, "UNWIRE <TARGET>.<PORT>")),
        "SET" if tokens.len() == 3 => {
            let sensor = normalize_id(tokens[1], input)?;
            let value = match tokens[2].to_ascii_uppercase().as_str() {
                "HIGH" => true,
                "LOW" => false,
                _ => return Err(usage(input, "SET <SENSOR_A|SENSOR_B> <HIGH|LOW>")),
            };
            Ok(DslCommand::Set { sensor, value })
        }
        "SET" => Err(usage(input, "SET <SENSOR_A|SENSOR_B> <HIGH|LOW>")),
        "INSPECT" if tokens.len() <= 2 => Ok(DslCommand::Inspect {
            id: tokens
                .get(1)
                .map(|token| normalize_id(token, input))
                .transpose()?,
        }),
        "INSPECT" => Err(usage(input, "INSPECT [ID]")),
        "CLEAR" if tokens.len() == 2 && tokens[1].eq_ignore_ascii_case("CIRCUIT") => {
            Ok(DslCommand::ClearCircuit)
        }
        "CLEAR" => Err(usage(input, "CLEAR CIRCUIT")),
        _ => Err(diagnostic(
            "unknown_command",
            format!("unknown command '{}'; enter HELP for syntax", tokens[0]),
            locate(input, tokens[0]),
            locate(input, tokens[0]) + tokens[0].len(),
        )),
    }
}

pub fn normalize_id(token: &str, input: &str) -> Result<String, DslDiagnostic> {
    let normalized = token.to_ascii_uppercase();
    let valid = normalized.len() <= 32
        && normalized
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic())
        && normalized
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
    if valid {
        Ok(normalized)
    } else {
        let start = locate(input, token);
        Err(diagnostic(
            "invalid_identifier",
            "identifiers must match [A-Z][A-Z0-9_]{0,31}",
            start,
            start + token.len(),
        ))
    }
}

fn parse_gate(token: &str, input: &str) -> Result<GateKind, DslDiagnostic> {
    match token.to_ascii_uppercase().as_str() {
        "AND" => Ok(GateKind::And),
        "OR" => Ok(GateKind::Or),
        "NOT" => Ok(GateKind::Not),
        "XOR" => Ok(GateKind::Xor),
        _ => Err(usage(input, "CREATE <AND|OR|NOT|XOR> <ID>")),
    }
}

fn parse_target(token: &str, input: &str) -> Result<(String, String), DslDiagnostic> {
    let Some((target, port)) = token.rsplit_once('.') else {
        return Err(usage(
            input,
            "target must include a port, such as G_AND_01.A",
        ));
    };
    let target = normalize_id(target, input)?;
    let port = normalize_id(port, input)?;
    Ok((target, port))
}

fn exact(actual: usize, expected: usize, input: &str) -> Result<(), DslDiagnostic> {
    if actual == expected {
        Ok(())
    } else {
        Err(usage(input, "command has unexpected arguments"))
    }
}

fn usage(input: &str, message: &str) -> DslDiagnostic {
    diagnostic("invalid_syntax", message, 0, input.len())
}

fn diagnostic(
    code: impl Into<String>,
    message: impl Into<String>,
    start: usize,
    end: usize,
) -> DslDiagnostic {
    DslDiagnostic {
        code: code.into(),
        message: message.into(),
        start,
        end,
    }
}

fn locate(input: &str, token: &str) -> usize {
    input.find(token).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_case_insensitive_commands_and_normalizes_ids() {
        assert_eq!(
            parse("  wire sensor_a -> gate_1.a ").unwrap(),
            DslCommand::Wire {
                source: "SENSOR_A".into(),
                target: "GATE_1".into(),
                port: "A".into(),
            }
        );
        assert_eq!(
            parse("create xor parity").unwrap(),
            DslCommand::Create {
                kind: GateKind::Xor,
                id: "PARITY".into(),
            }
        );
    }

    #[test]
    fn reports_stable_diagnostics_with_spans() {
        let error = parse("CREATE NAND gate").unwrap_err();
        assert_eq!(error.code, "invalid_syntax");
        let error = parse("REMOVE 9bad").unwrap_err();
        assert_eq!(error.code, "invalid_identifier");
        assert_eq!(&"REMOVE 9bad"[error.start..error.end], "9bad");
    }
}
