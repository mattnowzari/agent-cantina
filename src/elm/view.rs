use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, Tabs,
        ScrollbarOrientation, ScrollbarState, Wrap,
    },
};
use textwrap::Options;

use crate::theme::ElasticTheme;

use super::{
    Model,
    model::{
        ActivePanel, ChatRole, CreateAgentField, CreateAgentModal, CreateAgentTab, Modal, RunState,
    },
};

pub fn view(frame: &mut Frame, model: &mut Model) {
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
        .fg(ElasticTheme::ACCENT_SECONDARY)
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
        "Prompts ({})  [Tab switch] [Ctrl+S save] [Ctrl+R reload] [←/→/↑/↓ move]",
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

    // Record viewport dimensions for editor scroll logic.
    model.prompts_viewport_width = top_content_area.width;
    model.prompts_viewport_height = top_content_area.height;

    let prompts_display = if model.active == ActivePanel::Top {
        with_caret(&model.prompts_raw, model.prompts_cursor)
    } else {
        model.prompts_raw.clone()
    };

    let prompts_wrapped = wrap_preserve_newlines(&prompts_display, top_content_area.width);
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
        "Agents  [Tab switch] [↑/↓ select] [Enter choose+run] [n new] [g reload]  selected: {}",
        selected_agent_label
    );
    let agents_block = Block::default()
        .title(agents_title)
        .borders(Borders::ALL)
        .border_style(agents_border);

    if !model.config.is_ready() {
        let w = Paragraph::new("Waiting for env (KIBANA_URL/ES_HOST and API_KEY/ES_API_KEY)…")
            .wrap(Wrap { trim: false });
        let inner = agents_block.inner(agents);
        frame.render_widget(agents_block, agents);
        frame.render_widget(w, inner);
    } else if !model.prompts_loaded {
        let w = Paragraph::new("Waiting for PROMPTS.md…")
            .wrap(Wrap { trim: false });
        let inner = agents_block.inner(agents);
        frame.render_widget(agents_block, agents);
        frame.render_widget(w, inner);
    } else if model.agents_loading {
        let w = Paragraph::new("Loading agents from Agent Builder…")
            .wrap(Wrap { trim: false });
        let inner = agents_block.inner(agents);
        frame.render_widget(agents_block, agents);
        frame.render_widget(w, inner);
    } else if let Some(err) = &model.agents_error {
        let w = Paragraph::new(format!("Failed to load agents:\n\n{err}"))
            .wrap(Wrap { trim: false });
        let inner = agents_block.inner(agents);
        frame.render_widget(agents_block, agents);
        frame.render_widget(w, inner);
    } else if model.agents.is_empty() {
        let w = Paragraph::new("No agents loaded. Press 'g' to reload.")
            .wrap(Wrap { trim: false });
        let inner = agents_block.inner(agents);
        frame.render_widget(agents_block, agents);
        frame.render_widget(w, inner);
    } else {
        let agents_inner = agents_block.inner(agents);
        frame.render_widget(agents_block, agents);
        let agents_content_area = if agents_inner.width >= 2 {
            ratatui::layout::Rect {
                x: agents_inner.x,
                y: agents_inner.y,
                width: agents_inner.width.saturating_sub(1),
                height: agents_inner.height,
            }
        } else {
            agents_inner
        };

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
                            Style::default()
                                .fg(ElasticTheme::SUBTLE)
                                .add_modifier(Modifier::ITALIC),
                        )));
                    }
                }
                ListItem::new(lines)
            })
            .collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(ElasticTheme::WARNING)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        // Render with persistent state so ratatui can maintain offset for natural scrolling.
        frame.render_stateful_widget(list, agents_content_area, &mut model.agents_list_state);

        // Scrollbar (right side of agents pane)
        if agents_inner.width >= 2 && agents_inner.height > 0 && !model.agents.is_empty() {
            let sb_area = ratatui::layout::Rect {
                x: agents_inner.x + agents_inner.width - 1,
                y: agents_inner.y,
                width: 1,
                height: agents_inner.height,
            };

            // Best-effort: treat each agent as one "row" for scrollbar purposes.
            let pos = model.agents_list_state.selected().unwrap_or(0);
            let mut state = ScrollbarState::new(model.agents.len())
                .position(pos)
                .viewport_content_length(agents_content_area.height as usize);

            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(ElasticTheme::SUBTLE))
                .thumb_style(
                    Style::default()
                        .fg(ElasticTheme::SUBTLE)
                        .add_modifier(Modifier::BOLD),
                );

            frame.render_stateful_widget(scrollbar, sb_area, &mut state);
        }
    }

    // Build wrapped conversation lines based on the actual render width.
    let (run_hint, run_hint_style) = match model.run_state {
        RunState::Idle => ("[r run] ", Style::default().fg(ElasticTheme::PRIMARY)),
        RunState::Running => ("[running] ", Style::default().fg(ElasticTheme::WARNING)),
        RunState::Done => ("[r run again] ", Style::default().fg(ElasticTheme::SUCCESS)),
        RunState::Error => ("[r retry] ", Style::default().fg(ElasticTheme::DANGER)),
    };
    let bottom_title = Line::from(vec![
        Span::raw("Conversation  "),
        Span::styled(run_hint, run_hint_style.add_modifier(Modifier::BOLD)),
        Span::raw("[Esc quit] [↑/↓ scroll] [End bottom]"),
    ]);
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
            .style(Style::default().fg(ElasticTheme::SUBTLE))
            .thumb_style(
                Style::default()
                    .fg(ElasticTheme::SUBTLE)
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
            .style(Style::default().fg(ElasticTheme::SUBTLE))
            .thumb_style(
                Style::default()
                    .fg(ElasticTheme::SUBTLE)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_stateful_widget(scrollbar, sb_area, &mut state);
    }

    if let Some(modal) = model.modal.as_mut() {
        render_modal(frame, modal);
    }
}

fn with_caret(s: &str, cursor: usize) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let idx = cursor.min(s.len());
    out.push_str(&s[..idx]);
    // Use a 1-column ASCII cursor to avoid ambiguous-width glyph spacing.
    out.push('|');
    out.push_str(&s[idx..]);
    out
}

fn selected_agent_label(model: &Model) -> String {
    if let Some(id) = model.selected_agent_id.as_deref() {
        if let Some(a) = model.agents.iter().find(|a| a.id == id) {
            return a.name.to_string();
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
                    .fg(ElasticTheme::SUBTLE)
                    .add_modifier(Modifier::ITALIC),
            ),
            ChatRole::User => ("[you]", Style::default().fg(ElasticTheme::ACCENT)),
            ChatRole::Agent => ("[agent]", Style::default().fg(ElasticTheme::ACCENT_SECONDARY)),
        };
        let is_user = entry.role == ChatRole::User;

        let label_text = format!("[{}] {}", entry.timestamp, label);
        let mut label_line = Line::from(vec![Span::styled(label_text, style)]);
        if is_user {
            label_line = label_line.right_aligned();
        }
        out.push(label_line);

        if matches!(entry.role, ChatRole::User | ChatRole::Agent) {
            out.extend(bubble_lines(&entry.content, width, style, is_user));
        } else {
            for raw in entry.content.lines() {
                for wrapped in wrap_one_line(raw, width) {
                    let mut line = Line::from(Span::raw(wrapped));
                    if is_user {
                        line = line.right_aligned();
                    }
                    out.push(line);
                }
            }
        }
        out.push(Line::from(""));
    }

    if model.waiting_for_response {
        let spinner = spinner_char(model.spinner_frame);
        let style = Style::default()
            .fg(ElasticTheme::WARNING)
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

fn bubble_lines(content: &str, width: u16, border_style: Style, align_right: bool) -> Vec<Line<'static>> {
    let total_w = width as usize;
    if total_w < 4 {
        // Too narrow to draw a box; fall back to plain wrapping.
        return wrap_preserve_newlines(content, width)
            .into_iter()
            .map(|s| Line::from(Span::raw(s)))
            .collect();
    }

    // Leave a tiny margin so bubbles don't touch the pane border.
    let margin = 1usize;
    let max_bubble_w = total_w.saturating_sub(margin).max(4);
    let max_inner_w = max_bubble_w.saturating_sub(2).max(1);

    // Wrap content to the maximum inner width.
    let mut wrapped_lines: Vec<String> = Vec::new();
    for line in content.lines() {
        wrapped_lines.extend(wrap_one_line(line, max_inner_w.min(u16::MAX as usize) as u16));
    }
    if content.ends_with('\n') {
        wrapped_lines.push(String::new());
    }
    if wrapped_lines.is_empty() {
        wrapped_lines.push(String::new());
    }

    let longest = wrapped_lines
        .iter()
        .map(|s| s.chars().count())
        .max()
        .unwrap_or(1);
    let inner_w = longest.clamp(1, max_inner_w);
    let bubble_w = inner_w + 2;

    let left_pad = if align_right {
        total_w.saturating_sub(bubble_w)
    } else {
        0
    };
    let pad = " ".repeat(left_pad);

    let mut out: Vec<Line<'static>> = Vec::new();

    let top = Line::from(vec![
        Span::raw(pad.clone()),
        Span::styled("┌".to_string(), border_style),
        Span::styled("─".repeat(inner_w), border_style),
        Span::styled("┐".to_string(), border_style),
    ]);
    out.push(top);

    for line in wrapped_lines {
        let len = line.chars().count();
        let mut body = line;
        if len < inner_w {
            body.push_str(&" ".repeat(inner_w - len));
        }
        out.push(Line::from(vec![
            Span::raw(pad.clone()),
            Span::styled("│".to_string(), border_style),
            Span::raw(body),
            Span::styled("│".to_string(), border_style),
        ]));
    }

    let bottom = Line::from(vec![
        Span::raw(pad),
        Span::styled("└".to_string(), border_style),
        Span::styled("─".repeat(inner_w), border_style),
        Span::styled("┘".to_string(), border_style),
    ]);
    out.push(bottom);

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

fn render_modal(frame: &mut Frame, modal: &mut Modal) {
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

    frame.render_widget(Clear, rect);
    match modal {
        Modal::CreateAgent(state) => render_create_agent_modal(frame, rect, state),
        Modal::MissingEnv { missing } => {
            let title = "Missing env vars";
            let message = format!(
                "Set these env vars and restart:\n\n{}\n\nPress Enter/Esc to dismiss.",
                missing.join(", ")
            );
            let border_style = Style::default()
                .fg(ElasticTheme::DANGER)
                .add_modifier(Modifier::BOLD);

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
        Modal::Info { title, message } => {
            let message = format!("{message}\n\nPress Enter/Esc to dismiss.");
            let border_style = Style::default().fg(ElasticTheme::PRIMARY);

            let widget = Paragraph::new(message)
                .block(
                    Block::default()
                        .title(title.as_str())
                        .borders(Borders::ALL)
                        .border_style(border_style),
                )
                .wrap(Wrap { trim: false });

            frame.render_widget(widget, rect);
        }
        Modal::Error { title, message } => {
            let message = format!("{message}\n\nPress Enter/Esc to dismiss.");
            let border_style = Style::default()
                .fg(ElasticTheme::DANGER)
                .add_modifier(Modifier::BOLD);

            let widget = Paragraph::new(message)
                .block(
                    Block::default()
                        .title(title.as_str())
                        .borders(Borders::ALL)
                        .border_style(border_style),
                )
                .wrap(Wrap { trim: false });

            frame.render_widget(widget, rect);
        }
    }
}

fn render_create_agent_modal(
    frame: &mut Frame,
    rect: ratatui::layout::Rect,
    state: &mut CreateAgentModal,
) {
    let block = Block::default()
        .title("Create agent")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ElasticTheme::ACCENT_SECONDARY).add_modifier(Modifier::BOLD));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let [tabs_area, content_area, help_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(3),
    ])
    .areas(inner);

    let tabs = Tabs::new(vec![
        Line::from("Prompt"),
        Line::from(format!("Tools ({})", state.selected_tool_ids.len())),
    ])
    .select(match state.tab {
        CreateAgentTab::Prompt => 0,
        CreateAgentTab::Tools => 1,
    })
    .style(Style::default().fg(ElasticTheme::SUBTLE))
    .highlight_style(Style::default().fg(ElasticTheme::ACCENT).add_modifier(Modifier::BOLD))
    .divider(" | ");
    frame.render_widget(tabs, tabs_area);

    let [name_area, desc_area, prompt_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Fill(1),
    ])
    .areas(content_area);

    let focused = Style::default()
        .fg(ElasticTheme::ACCENT)
        .add_modifier(Modifier::BOLD);
    let normal = Style::default().fg(ElasticTheme::SUBTLE);

    let name_border = if state.focus == CreateAgentField::Name { focused } else { normal };
    let desc_border =
        if state.focus == CreateAgentField::Description { focused } else { normal };
    let prompt_border =
        if state.focus == CreateAgentField::Instructions { focused } else { normal };

    let name_text = if state.focus == CreateAgentField::Name {
        format!("{}▍", state.name)
    } else {
        state.name.clone()
    };
    let desc_text = if state.focus == CreateAgentField::Description {
        format!("{}▍", state.description)
    } else {
        state.description.clone()
    };
    let prompt_text = if state.focus == CreateAgentField::Instructions {
        format!("{}▍", state.instructions)
    } else {
        state.instructions.clone()
    };

    let name_widget = Paragraph::new(name_text)
        .block(Block::default().title("Name").borders(Borders::ALL).border_style(name_border))
        .wrap(Wrap { trim: false });
    let desc_widget = Paragraph::new(desc_text)
        .block(
            Block::default()
                .title("Description (optional)")
                .borders(Borders::ALL)
                .border_style(desc_border),
        )
        .wrap(Wrap { trim: false });

    // Prompt widget (bottom-aligned as it grows).
    let wrapped = wrap_preserve_newlines(&prompt_text, prompt_area.width.saturating_sub(2));
    let total_lines = wrapped.len();
    let viewport_h = prompt_area.height.saturating_sub(2) as usize;
    let scroll_from_top = total_lines.saturating_sub(viewport_h) as u16;
    let prompt_widget = Paragraph::new(Text::from(
        wrapped
            .into_iter()
            .map(Line::from)
            .collect::<Vec<Line<'static>>>(),
    ))
    .scroll((scroll_from_top, 0))
    .block(
        Block::default()
            .title("Instructions / prompt")
            .borders(Borders::ALL)
            .border_style(prompt_border),
    )
    .wrap(Wrap { trim: false });

    // Tools tab content
    let tools_border = if state.tab == CreateAgentTab::Tools { focused } else { normal };
    let tools_area = content_area;

    let mut help_lines: Vec<Line> = vec![Line::from(vec![
        Span::styled("[←/→]", Style::default().fg(ElasticTheme::PRIMARY).add_modifier(Modifier::BOLD)),
        Span::raw(" switch tab  "),
        Span::styled("[Tab]", Style::default().fg(ElasticTheme::PRIMARY).add_modifier(Modifier::BOLD)),
        Span::raw(" next field  "),
        Span::styled("[Enter]", Style::default().fg(ElasticTheme::PRIMARY).add_modifier(Modifier::BOLD)),
        Span::raw(" newline (prompt) / toggle (tools)  "),
        Span::styled("[Ctrl+S]", Style::default().fg(ElasticTheme::SUCCESS).add_modifier(Modifier::BOLD)),
        Span::raw(" create  "),
        Span::styled("[Esc]", Style::default().fg(ElasticTheme::DANGER).add_modifier(Modifier::BOLD)),
        Span::raw(" cancel"),
    ])];

    if state.submitting {
        help_lines.push(Line::from(vec![
            Span::styled("Creating…", Style::default().fg(ElasticTheme::WARNING).add_modifier(Modifier::ITALIC)),
        ]));
    } else if let Some(err) = &state.error {
        help_lines.push(Line::from(vec![Span::styled(
            err.clone(),
            Style::default().fg(ElasticTheme::DANGER).add_modifier(Modifier::BOLD),
        )]));
    } else {
        help_lines.push(Line::from(vec![Span::styled(
            "Tools tab: ↑/↓ + Space to toggle, A=all, X=none",
            Style::default().fg(ElasticTheme::SUBTLE).add_modifier(Modifier::ITALIC),
        )]));
    }

    let help_widget = Paragraph::new(Text::from(help_lines))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(ElasticTheme::SUBTLE)))
        .wrap(Wrap { trim: false });

    if state.tab == CreateAgentTab::Prompt {
        frame.render_widget(name_widget, name_area);
        frame.render_widget(desc_widget, desc_area);
        frame.render_widget(prompt_widget, prompt_area);
    } else {
        // Tools list fills the content area when on Tools tab.
        let block = Block::default()
            .title("Tools (Space toggle, A all, X none)")
            .borders(Borders::ALL)
            .border_style(tools_border);
        let inner = block.inner(tools_area);
        frame.render_widget(block, tools_area);

        if state.tools_loading {
            let w = Paragraph::new("Loading tools…")
                .wrap(Wrap { trim: false });
            frame.render_widget(w, inner);
        } else if let Some(err) = &state.tools_error {
            let w = Paragraph::new(format!("Failed to load tools:\n\n{err}"))
                .wrap(Wrap { trim: false });
            frame.render_widget(w, inner);
        } else if state.tools.is_empty() {
            let w = Paragraph::new("No tools returned by Agent Builder.")
                .wrap(Wrap { trim: false });
            frame.render_widget(w, inner);
        } else {
            let items: Vec<ListItem> = state
                .tools
                .iter()
                .map(|t| {
                    let checked = state.selected_tool_ids.iter().any(|id| id == &t.id);
                    let head = format!("{} {}", if checked { "[x]" } else { "[ ]" }, t.id);
                    let mut lines = vec![Line::from(head)];

                    // Render metadata so ToolSummary fields are actually used.
                    let mut meta_bits: Vec<String> = Vec::new();
                    if !t.tool_type.trim().is_empty() {
                        meta_bits.push(t.tool_type.trim().to_string());
                    }
                    if t.readonly {
                        meta_bits.push("readonly".to_string());
                    }
                    if !t.tags.is_empty() {
                        meta_bits.push(format!("tags: {}", t.tags.join(", ")));
                    }
                    if !meta_bits.is_empty() {
                        lines.push(Line::from(Span::styled(
                            meta_bits.join(" • "),
                            Style::default()
                                .fg(ElasticTheme::SUBTLE)
                                .add_modifier(Modifier::ITALIC),
                        )));
                    }

                    if !t.description.trim().is_empty() {
                        lines.push(Line::from(Span::styled(
                            t.description.trim().to_string(),
                            Style::default()
                                .fg(ElasticTheme::SUBTLE)
                                .add_modifier(Modifier::ITALIC),
                        )));
                    }
                    ListItem::new(lines)
                })
                .collect();

            let list = List::new(items)
                .highlight_style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(ElasticTheme::WARNING)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("▶ ");

            frame.render_stateful_widget(list, inner, &mut state.tools_list_state);
        }
    }
    frame.render_widget(help_widget, help_area);
}
