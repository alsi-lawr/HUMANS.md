use super::{Flow, Stage, Treatment, canvas::Chart};
use ratatui::{layout::Rect, text::Line};

pub(super) fn render(flow: &Flow, width: u16) -> Vec<Line<'static>> {
    prepare(flow).lines(width)
}

fn prepare(flow: &Flow) -> Chart {
    let mut chart = match flow.treatment {
        Treatment::Pipeline => pipeline(flow),
        Treatment::Dialogue => dialogue(flow),
        Treatment::Linear | Treatment::Review | Treatment::Correction => sequence(flow),
    };
    if flow.preflight {
        let x = if flow.treatment == Treatment::Pipeline {
            4
        } else {
            10
        };
        let y = chart.height + 1;
        chart.text(
            x,
            y,
            if flow.treatment == Treatment::Pipeline {
                "During Implement N · optional"
            } else {
                "During Implement · optional"
            },
        );
        chart.node(
            x,
            y + 1,
            Stage::Preflight.label(),
            flow.model(Stage::Preflight),
        );
    }
    chart
}

fn sequence(flow: &Flow) -> Chart {
    let mut chart = Chart::new();
    let x = if flow.treatment == Treatment::Review {
        4
    } else {
        10
    };
    let mut path = Vec::new();
    let mut y = 0;
    for (index, stage) in flow.main.iter().enumerate() {
        let area = chart.node(x, y, stage.label(), flow.model(*stage));
        if index + 1 < flow.main.len() {
            chart.down(area);
        }
        y = area.bottom() + 1;
        path.push(area);
    }
    match flow.treatment {
        Treatment::Correction => {
            let source = path[path.len() - 2].y + 1;
            chart.put(32, 1, "◀─────────────┐");
            chart.vertical(46, 2, source);
            chart.put(32, source, "──────────────┘");
            chart.text(48, source.div_ceil(2), "Fix");
        }
        Treatment::Review => revision(&mut chart, path[path.len() - 2]),
        Treatment::Linear => {}
        Treatment::Pipeline | Treatment::Dialogue => {
            unreachable!("branched treatments have dedicated layouts")
        }
    }
    chart
}

fn pipeline(flow: &Flow) -> Chart {
    let mut chart = Chart::new();
    let writer = chart.node(17, 0, "Implement N", flow.model(Stage::Implement));
    let fork = writer.bottom();
    chart.put(28, fork - 1, "┬");
    chart.put(15, fork, "┌────────────┴────────────┐");
    chart.put(15, fork + 1, "│");
    chart.text(34, fork + 1, "if independent");
    chart.put(15, fork + 2, "▼");
    chart.put(41, fork + 2, "▼");
    chart.node(30, fork + 3, "Implement N+1", flow.model(Stage::Implement));
    let mut current = fork + 3;
    let mut review = 0;
    for (index, stage) in flow.main.iter().skip(1).enumerate() {
        let area = chart.node(
            4,
            current,
            &format!("{} N", stage.label()),
            flow.model(*stage),
        );
        if index + 2 < flow.main.len() {
            chart.down(area);
            review = area.y + 1;
        }
        current = area.bottom() + 1;
    }
    chart.put(1, 1, "┌──────────────▶");
    chart.vertical(1, 2, review);
    chart.text(3, fork + 1, "Fix");
    chart.put(1, review, "└──");
    chart
}

fn dialogue(flow: &Flow) -> Chart {
    let mut chart = Chart::new();
    let chair = chart.node(4, 0, "Chair", flow.model(Stage::Chair));
    let challenger = chart.node(
        30,
        chair.bottom() + 2,
        "Challenger",
        flow.model(Stage::Challenger),
    );
    chart.put(26, 1, "───────────────┐");
    chart.vertical(41, 2, challenger.y - 1);
    chart.text(43, 3, "spawn");
    chart.put(41, challenger.y - 1, "▼");
    chart.put(15, chair.bottom() - 1, "┬");
    chart.vertical(15, chair.bottom(), challenger.bottom());
    chart.put(41, challenger.bottom() - 1, "┬");
    chart.put(15, challenger.bottom(), "├─────────────────────────┘");
    chart.put(15, challenger.bottom() + 1, "▼");
    let reconcile = chart.node(4, challenger.bottom() + 2, "Reconcile", None);
    chart.down(reconcile);
    chart.node(4, reconcile.bottom() + 1, "Done", None);
    revision(&mut chart, reconcile);
    chart
}

fn revision(chart: &mut Chart, source: Rect) {
    chart.text(26, source.y, "Fix");
    chart.put(26, source.y + 1, "───▶");
    chart.node(30, source.y, "Revise", None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy_flow::{Runtime, contracts};
    use casefile_core::parse_strategy_projection;
    use casefile_store::{
        DerivedStrategy, EffectiveWriterBinding, StrategyBindingState, WriterBindingSource,
    };
    use ratatui::text::Span;

    #[test]
    fn wide_model_identifiers_wrap_inside_closed_nonoverlapping_boxes() {
        let source =
            include_str!("../../../adapters/codex/matrices/casefile-implement-pipeline.toml");
        let identifier = "模型-e\u{301}-runtime-".repeat(12);
        let mut strategy = DerivedStrategy {
            matrix: parse_strategy_projection("implementation.toml", source)
                .unwrap()
                .unwrap(),
            binding: Some(StrategyBindingState::Resolved {
                effective: EffectiveWriterBinding {
                    model: identifier.clone(),
                    reasoning_effort: "high".into(),
                    source: WriterBindingSource::Binding,
                },
            }),
        };
        for worker in &mut strategy.matrix.workers {
            worker.model = Some(identifier.clone());
        }
        let flow =
            contracts::build("casefile-implement-pipeline", "implementation", &strategy).unwrap();
        let chart = prepare(&flow);
        let buffer = chart.buffer();
        for (index, node) in chart.nodes.iter().enumerate() {
            let area = node.area;
            for other in chart.nodes.iter().skip(index + 1) {
                assert!(area.intersection(other.area).is_empty());
            }
            for y in area.y + 1..area.bottom() - 1 {
                assert_eq!(buffer[(area.x, y)].symbol(), "│");
                assert_eq!(buffer[(area.right() - 1, y)].symbol(), "│");
            }
            if area.height > 3 {
                let mut actual = String::new();
                for y in area.y + 2..area.bottom() - 1 {
                    let mut x = area.x + 1;
                    while x < area.right() - 1 {
                        let value = buffer[(x, y)].symbol();
                        actual.extend(value.chars().filter(|character| !character.is_whitespace()));
                        x += Span::raw(value).width().max(1) as u16;
                    }
                }
                assert_eq!(actual, identifier);
            }
        }
        for width in [54, 78] {
            assert!(
                chart
                    .lines(width)
                    .iter()
                    .all(|line| line.width() <= usize::from(width))
            );
        }
        assert_eq!(
            flow.model(Stage::Implement),
            Some(&Runtime::Model(identifier))
        );
    }
}
