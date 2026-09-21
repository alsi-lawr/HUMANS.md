use super::{Flow, Node};
use crate::ui::{ACCENT, MUTED};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

pub(super) fn wrapped(text: &str, width: u16) -> Vec<Line<'static>> {
    let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
    let height = paragraph.line_count(width).min(u16::MAX as usize) as u16;
    let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
    paragraph.render(buffer.area, &mut buffer);
    lines(&buffer)
}

struct Placed<'a> {
    node: &'a Node,
    row: usize,
    area: Rect,
}
struct Route {
    from: usize,
    to: usize,
    lane: Option<usize>,
    departure: u16,
    arrival: u16,
}
struct Drawing<'a> {
    nodes: Vec<Placed<'a>>,
    wires: Vec<Vec<(usize, u8)>>,
    arrows: Vec<(u16, u16, &'static str)>,
    area: Rect,
}

pub(super) fn render(flow: &Flow, width: u16) -> Vec<Line<'static>> {
    if width < 16 {
        return wrapped("Pane too narrow", width.max(1));
    }
    let Some(drawing) = prepare(flow, width) else {
        return wrapped("Pane too narrow", width.max(1));
    };
    let mut buffer = Buffer::empty(drawing.area);
    for y in 0..drawing.area.height {
        for x in 0..width {
            let wires = &drawing.wires[usize::from(y) * usize::from(width) + usize::from(x)];
            let mask = wires.iter().fold(0, |mask, (_, part)| mask | part);
            if mask != 0 {
                let crossing = wires.iter().any(|(a, _)| {
                    wires
                        .iter()
                        .any(|(b, _)| !connected(flow.edges[*a], flow.edges[*b]))
                });
                // A dotted underpass is not a junction between independent routes.
                let symbol = if crossing { "┆" } else { glyph(mask) };
                buffer[(x, y)]
                    .set_symbol(symbol)
                    .set_style(Style::default().fg(MUTED));
            }
        }
    }
    for (x, y, symbol) in drawing.arrows {
        buffer[(x, y)]
            .set_symbol(symbol)
            .set_style(Style::default().fg(ACCENT));
    }
    for placed in drawing.nodes {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(MUTED));
        let inner = block.inner(placed.area);
        block.render(placed.area, &mut buffer);
        Paragraph::new(placed.node.label.as_str())
            .wrap(Wrap { trim: false })
            .render(inner, &mut buffer);
    }
    lines(&buffer)
}

fn connected(a: (super::Stage, super::Stage), b: (super::Stage, super::Stage)) -> bool {
    a.0 == b.0 || a.1 == b.1
}

fn prepare(flow: &Flow, width: u16) -> Option<Drawing<'_>> {
    let rows = flow
        .rows
        .iter()
        .flat_map(|row| {
            if width < 44 {
                row.iter().map(|node| vec![node]).collect::<Vec<_>>()
            } else {
                vec![row.iter().collect()]
            }
        })
        .collect::<Vec<_>>();
    let mut nodes = rows
        .iter()
        .enumerate()
        .flat_map(|(row, nodes)| {
            nodes.iter().map(move |node| Placed {
                node,
                row,
                area: Rect::default(),
            })
        })
        .collect::<Vec<_>>();
    let mut lanes: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut gaps = vec![0u16; rows.len()];
    let mut routes = Vec::new();
    for (from, to) in &flow.edges {
        let from = nodes
            .iter()
            .position(|node| node.node.stage == *from)
            .expect("source stage");
        let to = nodes
            .iter()
            .position(|node| node.node.stage == *to)
            .expect("target stage");
        let a = nodes[from].row;
        let b = nodes[to].row;
        let lane = if b == a + 1 {
            None
        } else {
            let range = (a.min(b), a.max(b));
            let lane = lanes
                .iter()
                .position(|used| {
                    used.iter()
                        .all(|(start, end)| range.1 < *start || range.0 > *end)
                })
                .unwrap_or(lanes.len());
            if lane == lanes.len() {
                lanes.push(Vec::new());
            }
            lanes[lane].push(range);
            Some(lane)
        };
        routes.push(Route {
            from,
            to,
            lane,
            departure: 0,
            arrival: 0,
        });
    }
    for (row, gap) in gaps.iter_mut().enumerate() {
        let mut direct_sources = Vec::new();
        let mut departures = routes
            .iter_mut()
            .filter(|route| nodes[route.from].row == row)
            .collect::<Vec<_>>();
        // Leave through lateral rails before another source enters that column.
        departures.sort_by_key(|route| route.lane.is_none());
        for route in departures {
            route.departure = if route.lane.is_none() {
                if let Some((_, slot)) = direct_sources
                    .iter()
                    .find(|(source, _)| *source == route.from)
                {
                    *slot
                } else {
                    let slot = *gap;
                    *gap += 1;
                    direct_sources.push((route.from, slot));
                    slot
                }
            } else {
                let slot = *gap;
                *gap += 1;
                slot
            };
        }
        for route in routes
            .iter_mut()
            .filter(|route| route.lane.is_some() && nodes[route.to].row == row + 1)
        {
            route.arrival = *gap;
            *gap += 1;
        }
    }
    let gutter = 2 + lanes.len() as u16 * 2;
    let available = width.saturating_sub(gutter + 1);
    if available < 4 {
        return None;
    }
    let mut y = 0;
    let mut bottoms = Vec::new();
    for (row, row_nodes) in rows.iter().enumerate() {
        let count = row_nodes.len() as u16;
        let node_width = (available.saturating_sub((count - 1) * 2) / count).max(4);
        let mut height = 0;
        for (column, node) in nodes.iter_mut().filter(|node| node.row == row).enumerate() {
            let node_height = Paragraph::new(node.node.label.as_str())
                .wrap(Wrap { trim: false })
                .line_count(node_width - 2) as u16
                + 2;
            height = height.max(node_height);
            node.area = Rect::new(
                gutter + column as u16 * (node_width + 2),
                y,
                node_width,
                node_height,
            );
        }
        bottoms.push(y + height);
        y += height + gaps[row] + 2;
    }
    let height = *bottoms.last().expect("flow has nodes");
    let mut drawing = Drawing {
        nodes,
        wires: vec![Vec::new(); usize::from(width) * usize::from(height)],
        arrows: Vec::new(),
        area: Rect::new(0, 0, width, height),
    };
    for (index, route) in routes.iter().enumerate() {
        let source = &drawing.nodes[route.from];
        let target = &drawing.nodes[route.to];
        let sx = source.area.x + source.area.width / 2;
        let tx = target.area.x + target.area.width / 2;
        let sy = source.area.bottom() - 1;
        let departure = bottoms[source.row] + route.departure;
        let ty = target.area.y - 1;
        if let Some(lane) = route.lane {
            let x = 1 + lane as u16 * 2;
            let arrival = bottoms[target.row - 1] + route.arrival;
            drawing.segment(index, (sx, sy), (sx, departure));
            drawing.segment(index, (sx, departure), (x, departure));
            drawing.segment(index, (x, departure), (x, arrival));
            drawing.segment(index, (x, arrival), (tx, arrival));
            drawing.arrows.push((tx - 1, arrival, "▶"));
            drawing.arrows.push((x + 1, departure, "◀"));
            drawing.segment(index, (tx, arrival), (tx, ty));
        } else {
            drawing.segment(index, (sx, sy), (sx, departure));
            drawing.segment(index, (sx, departure), (tx, departure));
            if sx != tx {
                drawing.arrows.push((
                    if sx < tx { tx - 1 } else { tx + 1 },
                    departure,
                    if sx < tx { "▶" } else { "◀" },
                ));
            }
            drawing.segment(index, (tx, departure), (tx, ty));
        }
        drawing.arrows.push((tx, ty, "▼"));
    }
    Some(drawing)
}

impl Drawing<'_> {
    fn segment(&mut self, edge: usize, from: (u16, u16), to: (u16, u16)) {
        let (mut x, mut y) = from;
        while (x, y) != to {
            let (next, outgoing, incoming) = if x < to.0 {
                ((x + 1, y), 2, 8)
            } else if x > to.0 {
                ((x - 1, y), 8, 2)
            } else if y < to.1 {
                ((x, y + 1), 4, 1)
            } else {
                ((x, y - 1), 1, 4)
            };
            self.mark(edge, (x, y), outgoing);
            self.mark(edge, next, incoming);
            (x, y) = next;
        }
    }
    fn mark(&mut self, edge: usize, (x, y): (u16, u16), mask: u8) {
        let wires = &mut self.wires[usize::from(y) * usize::from(self.area.width) + usize::from(x)];
        if let Some((_, part)) = wires.iter_mut().find(|(owner, _)| *owner == edge) {
            *part |= mask;
        } else {
            wires.push((edge, mask));
        }
    }
}

fn glyph(mask: u8) -> &'static str {
    match mask {
        1 | 4 | 5 => "│",
        2 | 8 | 10 => "─",
        3 => "└",
        6 => "┌",
        9 => "┘",
        12 => "┐",
        7 => "├",
        11 => "┴",
        13 => "┤",
        14 => "┬",
        15 => "┼",
        _ => " ",
    }
}

fn lines(buffer: &Buffer) -> Vec<Line<'static>> {
    (0..buffer.area.height)
        .map(|y| {
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut x = 0;
            while x < buffer.area.width {
                let cell = &buffer[(x, y)];
                let span = Span::styled(cell.symbol().to_owned(), cell.style());
                x += span.width().max(1) as u16;
                if let Some(previous) = spans.last_mut()
                    && previous.style == span.style
                {
                    previous.content.to_mut().push_str(&span.content);
                } else {
                    spans.push(span);
                }
            }
            Line::from(spans)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_parallel_routes_and_correction_returns_never_share_a_wire() {
        let source =
            include_str!("../../../adapters/codex/matrices/casefile-implement-pipeline.toml");
        let strategy = casefile_store::DerivedStrategy {
            matrix: casefile_core::parse_strategy_projection("implementation.toml", source)
                .unwrap()
                .unwrap(),
            binding: None,
        };
        let flow = super::super::contracts::build(
            "casefile-implement-pipeline",
            "implementation",
            &strategy,
        )
        .unwrap();
        for width in [30, 54, 78] {
            let drawing = prepare(&flow, width).unwrap();
            for wires in &drawing.wires {
                for (a, a_mask) in wires {
                    for (b, b_mask) in wires {
                        if !connected(flow.edges[*a], flow.edges[*b]) {
                            assert_eq!(
                                a_mask & b_mask,
                                0,
                                "unrelated edges {:?} and {:?} overlap at width {width}",
                                flow.edges[*a],
                                flow.edges[*b]
                            );
                        }
                    }
                }
            }
        }
    }
}
