use crate::modes::Item;
use anyhow::Result;
use gartk_core::{Rect, Theme};
use gartk_render::{Renderer, TextStyle};
use gartk_x11::Window;
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat};

/// The popup window UI
pub struct Popup {
    window: Window,
    renderer: Renderer,
    theme: Theme,
    prompt: String,
    gc: u32,
}

impl Popup {
    /// Create a new popup
    pub fn new(window: Window, prompt: Option<&str>) -> Result<Self> {
        let size = window.size();
        let theme = Theme::dark();
        let renderer = Renderer::with_theme(size.width, size.height, theme.clone())?;

        // Create a GC for blitting
        let conn = window.connection();
        let gc = conn.generate_id()?;
        conn.inner().create_gc(gc, window.id(), &Default::default())?;
        conn.flush()?;

        Ok(Self {
            window,
            renderer,
            theme,
            prompt: prompt.unwrap_or("Search:").to_string(),
            gc,
        })
    }

    /// Get the window
    pub fn window(&self) -> &Window {
        &self.window
    }

    /// Render the popup
    pub fn render(
        &mut self,
        input: &str,
        cursor: usize,
        items: &[&Item],
        selected: usize,
        total_count: usize,
    ) -> Result<()> {
        let size = self.renderer.size();
        let padding = self.theme.padding as i32;
        let item_height = (self.theme.font_size as i32) + (self.theme.item_padding as i32 * 2);

        // Clear background
        self.renderer.clear()?;

        // Draw border
        let border_rect = Rect::new(0, 0, size.width, size.height);
        self.renderer.stroke_rounded_rect(
            border_rect,
            self.theme.border_radius,
            self.theme.border,
            self.theme.border_width as f64,
        )?;

        // Calculate input field dimensions
        let input_field_height = (self.theme.font_size as i32) + (self.theme.item_padding as i32 * 2);
        let input_rect = Rect::new(
            padding,
            padding,
            size.width - (padding * 2) as u32,
            input_field_height as u32,
        );

        // Draw input area background
        self.renderer.fill_rounded_rect(
            input_rect,
            self.theme.border_radius / 2.0,
            self.theme.input_background,
        )?;

        // Draw prompt
        let prompt_style = TextStyle::new()
            .font_family(&self.theme.font_family)
            .font_size(self.theme.font_size)
            .color(self.theme.input_placeholder);

        let prompt_size = self.renderer.measure_text(&self.prompt, &prompt_style)?;

        // Calculate text vertical centering using actual measured height
        let text_y = padding + (input_field_height - prompt_size.height as i32) / 2;

        self.renderer.text(
            &self.prompt,
            (padding + self.theme.item_padding as i32) as f64,
            text_y as f64,
            &prompt_style,
        )?;

        // Draw input text
        let input_x = padding + self.theme.item_padding as i32 + prompt_size.width as i32 + 12;
        let input_style = TextStyle::new()
            .font_family(&self.theme.font_family)
            .font_size(self.theme.font_size)
            .color(self.theme.input_foreground);

        self.renderer.text(
            input,
            input_x as f64,
            text_y as f64,
            &input_style,
        )?;

        // Draw cursor - full height of input field with small vertical padding
        let cursor_text = &input[..cursor];
        let cursor_size = self.renderer.measure_text(cursor_text, &input_style)?;
        let cursor_x = input_x + cursor_size.width as i32;
        let cursor_v_padding = 6;
        let cursor_y = padding + cursor_v_padding;
        let cursor_height = input_field_height - (cursor_v_padding * 2);

        self.renderer.fill_rect(
            Rect::new(cursor_x, cursor_y, 2, cursor_height as u32),
            self.theme.input_cursor,
        )?;

        // Draw items starting below input (no separator line)
        let items_start_y = input_rect.bottom() + padding;

        for (i, item) in items.iter().enumerate() {
            let y = items_start_y + (i as i32 * item_height);
            let item_rect = Rect::new(
                padding,
                y,
                size.width - (padding * 2) as u32,
                item_height as u32,
            );

            // Highlight selected item
            if i == selected {
                self.renderer.fill_rounded_rect(
                    item_rect,
                    self.theme.border_radius / 2.0,
                    self.theme.item_selected_background,
                )?;
            }

            // Draw item name - vertically centered in item row
            let name_style = TextStyle::new()
                .font_family(&self.theme.font_family)
                .font_size(self.theme.font_size)
                .color(if i == selected {
                    self.theme.item_selected_foreground
                } else {
                    self.theme.item_foreground
                });

            // Measure actual text height for proper centering
            let name_size = self.renderer.measure_text(&item.name, &name_style)?;
            let item_text_y = y + (item_height - name_size.height as i32) / 2;

            self.renderer.text(
                &item.name,
                (padding + self.theme.item_padding as i32) as f64,
                item_text_y as f64,
                &name_style,
            )?;

            // Draw description if available (same vertical position as name)
            if let Some(desc) = &item.description {
                let desc_style = TextStyle::new()
                    .font_family(&self.theme.font_family)
                    .font_size(self.theme.font_size * 0.85)
                    .color(self.theme.item_description)
                    .ellipsize(true)
                    .max_width((size.width as i32 - padding * 2 - self.theme.item_padding as i32 * 2) / 2);

                let desc_x = padding + self.theme.item_padding as i32 + name_size.width as i32 + 16;

                // Slightly adjust description y to align baselines with smaller font
                let desc_size = self.renderer.measure_text(desc, &desc_style)?;
                let desc_y = y + (item_height - desc_size.height as i32) / 2;

                self.renderer.text(
                    desc,
                    desc_x as f64,
                    desc_y as f64,
                    &desc_style,
                )?;
            }
        }

        // Draw item count
        let count_text = format!("{}/{}", items.len(), total_count);
        let count_style = TextStyle::new()
            .font_family(&self.theme.font_family)
            .font_size(self.theme.font_size * 0.85)
            .color(self.theme.item_description);

        let count_size = self.renderer.measure_text(&count_text, &count_style)?;
        self.renderer.text(
            &count_text,
            (size.width as i32 - padding - count_size.width as i32) as f64,
            (size.height as i32 - padding - count_size.height as i32) as f64,
            &count_style,
        )?;

        // Flush and copy to window
        self.renderer.flush();
        self.blit_surface()?;

        Ok(())
    }

    /// Blit the rendered surface to the window
    fn blit_surface(&mut self) -> Result<()> {
        let size = self.renderer.size();
        let conn = self.window.connection();

        // Get the surface data by creating a temp surface and copying
        let ctx = self.renderer.context()?;
        ctx.target().flush();

        let mut temp_surface = gartk_render::Surface::new(size.width, size.height)?;
        let temp_ctx = temp_surface.context()?;
        temp_ctx.set_source_surface(self.renderer.surface().cairo_surface(), 0.0, 0.0)?;
        temp_ctx.paint()?;
        drop(temp_ctx);

        let data = temp_surface.data()?;

        conn.inner().put_image(
            ImageFormat::Z_PIXMAP,
            self.window.id(),
            self.gc,
            size.width as u16,
            size.height as u16,
            0,
            0,
            0,
            self.window.depth(),
            &data,
        )?;

        conn.flush()?;

        Ok(())
    }
}

impl Drop for Popup {
    fn drop(&mut self) {
        let _ = self.window.connection().inner().free_gc(self.gc);
    }
}
