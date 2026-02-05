use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
};

use super::{
    Model,
    model::{ActivePanel, ChatRole, Modal, RunState},
};

pub fn view(frame: &mut Frame, model: &Model) {
    let area = frame.area();

    let gap: u16 = 1;
    let [top, _spacer, bottom] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(gap),
        Constraint::Fill(1),
    ])
    .areas(area);

    let active_border = Style::default()
        .fg(Color::Rgb(255, 165, 0)) // orange
        .add_modifier(Modifier::BOLD);
    let inactive_border = Style::default();

    let top_border = if model.active == ActivePanel::Top {
        active_border
    } else {
        inactive_border
    };
    let bottom_border = if model.active == ActivePanel::Bottom {
        active_border
    } else {
        inactive_border
    };

    let top_title = format!(
        "Prompts ({})  [Tab switch] [j/k scroll]",
        model.prompts_path
    );
    let top_widget = Paragraph::new(model.prompts_raw.clone())
        .block(
            Block::default()
                .title(top_title)
                .borders(Borders::ALL)
                .border_style(top_border),
        )
        .wrap(Wrap { trim: false })
        .scroll((model.prompts_scroll, 0));

    let (chat_text, chat_lines) = chat_text(model);
    let inner_h = bottom.height.saturating_sub(2) as usize; // borders
    let max_scroll_from_top = chat_lines.saturating_sub(inner_h);
    let max_scroll_from_top = max_scroll_from_top.min(u16::MAX as usize) as u16;
    let from_bottom = model.chat_scroll_from_bottom.min(max_scroll_from_top);
    let chat_scroll_from_top = max_scroll_from_top.saturating_sub(from_bottom);
    let run_hint = match model.run_state {
        RunState::Idle => "[r run] ",
        RunState::Running => "[running] ",
        RunState::Done => "[r run again] ",
        RunState::Error => "[r retry] ",
    };
    let bottom_title = format!(
        "Conversation  {}[q quit] [j/k scroll] [End bottom]",
        run_hint
    );
    let bottom_block = Block::default()
        .title(bottom_title)
        .borders(Borders::ALL)
        .border_style(bottom_border);
    let bottom_inner = bottom_block.inner(bottom);
    let content_area = if bottom_inner.width >= 2 {
        ratatui::layout::Rect {
            x: bottom_inner.x,
            y: bottom_inner.y,
            width: bottom_inner.width.saturating_sub(1),
            height: bottom_inner.height,
        }
    } else {
        bottom_inner
    };

    let bottom_widget = Paragraph::new(chat_text)
        .wrap(Wrap { trim: false })
        .scroll((chat_scroll_from_top, 0));

    frame.render_widget(top_widget, top);
    frame.render_widget(bottom_block, bottom);
    frame.render_widget(bottom_widget, content_area);

    // Scrollbar (right side of conversation pane)
    if bottom_inner.width >= 2 && bottom_inner.height > 0 && chat_lines > 0 {
        let sb_area = ratatui::layout::Rect {
            x: bottom_inner.x + bottom_inner.width - 1,
            y: bottom_inner.y,
            width: 1,
            height: bottom_inner.height,
        };

        let mut state = ScrollbarState::new(chat_lines)
            .position(chat_scroll_from_top as usize)
            .viewport_content_length(bottom_inner.height as usize);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(Color::DarkGray))
            .thumb_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_stateful_widget(scrollbar, sb_area, &mut state);
    }

    if let Some(modal) = &model.modal {
        render_modal(frame, modal);
    }
}

fn chat_text(model: &Model) -> (Text<'static>, usize) {
    let mut lines: Vec<Line<'static>> = Vec::new();

    for entry in &model.chat {
        let (label, style) = match entry.role {
            ChatRole::System => (
                "[system]",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ),
            ChatRole::User => ("[you]", Style::default().fg(Color::Cyan)),
            ChatRole::Agent => ("[agent]", Style::default().fg(Color::Green)),
        };

        lines.push(Line::from(vec![Span::styled(label.to_string(), style)]));
        for l in entry.content.lines() {
            lines.push(Line::from(Span::raw(l.to_string())));
        }
        lines.push(Line::from(""));
    }

    if model.waiting_for_response {
        let spinner = spinner_char(model.spinner_frame);
        let style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::ITALIC);
        lines.push(Line::from(vec![Span::styled(
            format!("[agent] {spinner} waiting for response…"),
            style,
        )]));
        lines.push(Line::from(""));
    }

    let len = lines.len();
    (Text::from(lines), len)
}

fn spinner_char(frame: usize) -> &'static str {
    const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    FRAMES[frame % FRAMES.len()]
}

fn render_modal(frame: &mut Frame, modal: &Modal) {
    use ratatui::layout::Rect;

    let area = frame.area();
    let w = area.width.saturating_mul(3) / 4;
    let h = area.height.saturating_mul(2) / 3;
    let x = area.x + (area.width.saturating_sub(w) / 2);
    let y = area.y + (area.height.saturating_sub(h) / 2);
    let rect = Rect {
        x,
        y,
        width: w.max(20),
        height: h.max(8),
    };

    let (title, message, border_style) = match modal {
        Modal::MissingEnv { missing } => (
            "Missing env vars",
            format!(
                "Set these env vars and restart:\n\n{}\n\nPress Enter/Esc to dismiss.",
                missing.join(", ")
            ),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Modal::Info { title, message } => (
            title.as_str(),
            format!("{message}\n\nPress Enter/Esc to dismiss."),
            Style::default().fg(Color::Yellow),
        ),
        Modal::Error { title, message } => (
            title.as_str(),
            format!("{message}\n\nPress Enter/Esc to dismiss."),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
    };

    frame.render_widget(Clear, rect);
    let widget = Paragraph::new(message)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(widget, rect);
}
