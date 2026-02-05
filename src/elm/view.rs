use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Wrap,
    },
};
use textwrap::Options;

use super::{
    Model,
    model::{ActivePanel, ChatRole, Modal, RunState},
};

pub fn view(frame: &mut Frame, model: &Model) {
    let area = frame.area();

    let gap: u16 = 1;
    let agents_len: u16 = if area.height < 20 { 5 } else { 9 };
    let [top, _spacer1, agents, _spacer2, bottom] = Layout::vertical([
        Constraint::Fill(2),
        Constraint::Length(gap),
        Constraint::Length(agents_len),
        Constraint::Length(gap),
        Constraint::Fill(2),
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
    let agents_border = if model.active == ActivePanel::Agents {
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
        "Prompts ({})  [Tab switch] [↑/↓ scroll]",
        model.prompts_path
    );
    let top_block = Block::default()
        .title(top_title)
        .borders(Borders::ALL)
        .border_style(top_border);
    let top_inner = top_block.inner(top);
    let top_content_area = if top_inner.width >= 2 {
        ratatui::layout::Rect {
            x: top_inner.x,
            y: top_inner.y,
            width: top_inner.width.saturating_sub(1),
            height: top_inner.height,
        }
    } else {
        top_inner
    };

    let prompts_wrapped = wrap_preserve_newlines(&model.prompts_raw, top_content_area.width);
    let prompts_lines = prompts_wrapped.len();
    let prompts_inner_h = top_content_area.height as usize;
    let prompts_max_scroll_from_top = prompts_lines.saturating_sub(prompts_inner_h);
    let prompts_scroll_from_top =
        (model.prompts_scroll as usize).min(prompts_max_scroll_from_top);

    let top_widget = Paragraph::new(Text::from(
        prompts_wrapped
            .into_iter()
            .map(Line::from)
            .collect::<Vec<Line<'static>>>(),
    ))
    .scroll((prompts_scroll_from_top.min(u16::MAX as usize) as u16, 0));

    // Agents window
    let selected_agent_label = selected_agent_label(model);
    let agents_title = format!(
        "Agents  [Tab switch] [↑/↓ select] [Enter choose+run] [g reload]  selected: {}",
        selected_agent_label
    );
    let agents_block = Block::default()
        .title(agents_title)
        .borders(Borders::ALL)
        .border_style(agents_border);

    if !model.config.is_ready() {
        let w = Paragraph::new("Waiting for env (KIBANA_URL/ES_HOST and API_KEY/ES_API_KEY)…")
            .block(agents_block)
            .wrap(Wrap { trim: false });
        frame.render_widget(w, agents);
    } else if !model.prompts_loaded {
        let w = Paragraph::new("Waiting for PROMPTS.md…")
            .block(agents_block)
            .wrap(Wrap { trim: false });
        frame.render_widget(w, agents);
    } else if model.agents_loading {
        let w = Paragraph::new("Loading agents from Agent Builder…")
            .block(agents_block)
            .wrap(Wrap { trim: false });
        frame.render_widget(w, agents);
    } else if let Some(err) = &model.agents_error {
        let w = Paragraph::new(format!("Failed to load agents:\n\n{err}"))
            .block(agents_block)
            .wrap(Wrap { trim: false });
        frame.render_widget(w, agents);
    } else if model.agents.is_empty() {
        let w = Paragraph::new("No agents loaded. Press 'g' to reload.")
            .block(agents_block)
            .wrap(Wrap { trim: false });
        frame.render_widget(w, agents);
    } else {
        let items: Vec<ListItem> = model
            .agents
            .iter()
            .map(|a| {
                let mut lines = vec![Line::from(format!("{}  ({})", a.name, a.id))];
                if let Some(desc) = &a.description {
                    let desc = desc.trim();
                    if !desc.is_empty() {
                        lines.push(Line::from(Span::styled(
                            desc.to_string(),
                            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                        )));
                    }
                }
                ListItem::new(lines)
            })
            .collect();

        let list = List::new(items)
            .block(agents_block)
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        let mut state = ListState::default();
        let idx = model.agent_selected_index.min(model.agents.len().saturating_sub(1));
        state.select(Some(idx));
        frame.render_stateful_widget(list, agents, &mut state);
    }

    // Build wrapped conversation lines based on the actual render width.
    let run_hint = match model.run_state {
        RunState::Idle => "[r run] ",
        RunState::Running => "[running] ",
        RunState::Done => "[r run again] ",
        RunState::Error => "[r retry] ",
    };
    let bottom_title = format!(
        "Conversation  {}[q quit] [↑/↓ scroll] [End bottom]",
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

    let chat_lines_vec = chat_lines_wrapped(model, content_area.width);
    let chat_lines = chat_lines_vec.len();
    let inner_h = content_area.height as usize;
    let max_scroll_from_top = chat_lines.saturating_sub(inner_h);
    let max_scroll_from_top = max_scroll_from_top.min(u16::MAX as usize) as u16;
    let from_bottom = model.chat_scroll_from_bottom.min(max_scroll_from_top);
    let chat_scroll_from_top = max_scroll_from_top.saturating_sub(from_bottom);

    let bottom_widget = Paragraph::new(Text::from(chat_lines_vec))
        .scroll((chat_scroll_from_top, 0));

    frame.render_widget(top_block, top);
    frame.render_widget(top_widget, top_content_area);

    // Scrollbar (right side of prompts pane)
    if top_inner.width >= 2 && top_inner.height > 0 && prompts_lines > 0 {
        let sb_area = ratatui::layout::Rect {
            x: top_inner.x + top_inner.width - 1,
            y: top_inner.y,
            width: 1,
            height: top_inner.height,
        };

        let mut state = ScrollbarState::new(prompts_lines)
            .position(prompts_scroll_from_top)
            .viewport_content_length(top_content_area.height as usize);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(Color::DarkGray))
            .thumb_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_stateful_widget(scrollbar, sb_area, &mut state);
    }
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
            .viewport_content_length(content_area.height as usize);

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

fn selected_agent_label(model: &Model) -> String {
    if let Some(id) = model.selected_agent_id.as_deref() {
        if let Some(a) = model.agents.iter().find(|a| a.id == id) {
            return format!("{}", a.name);
        }
        return id.to_string();
    }
    "<none>".to_string()
}

fn chat_lines_wrapped(model: &Model, width: u16) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();

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
        let is_user = entry.role == ChatRole::User;

        let mut label_line = Line::from(vec![Span::styled(label.to_string(), style)]);
        if is_user {
            label_line = label_line.right_aligned();
        }
        out.push(label_line);

        for raw in entry.content.lines() {
            for wrapped in wrap_one_line(raw, width) {
                let mut line = Line::from(Span::raw(wrapped));
                if is_user {
                    line = line.right_aligned();
                }
                out.push(line);
            }
        }
        out.push(Line::from(""));
    }

    if model.waiting_for_response {
        let spinner = spinner_char(model.spinner_frame);
        let style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::ITALIC);
        out.push(
            Line::from(vec![Span::styled(
                format!("[agent] {spinner} waiting for response…"),
                style,
            )])
            .left_aligned(),
        );
        out.push(Line::from(""));
    }

    out
}

fn wrap_one_line(s: &str, width: u16) -> Vec<String> {
    let w = width as usize;
    if w == 0 {
        return vec![String::new()];
    }
    if s.is_empty() {
        return vec![String::new()];
    }

    let opts = Options::new(w).break_words(true);
    textwrap::wrap(s, &opts)
        .into_iter()
        .map(|c| c.into_owned())
        .collect()
}

fn wrap_preserve_newlines(s: &str, width: u16) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in s.lines() {
        out.extend(wrap_one_line(line, width));
    }
    // `lines()` drops the final empty line if the string ends with '\n'
    if s.ends_with('\n') {
        out.push(String::new());
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
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
