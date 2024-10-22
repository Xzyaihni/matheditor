use std::{cell::RefCell, mem};

use sdl2::{
    rect::Rect,
    ttf::Font,
    pixels::Color,
    render::{WindowCanvas, Texture, TextureCreator},
    video::WindowContext,
    event::{WindowEvent, Event},
    keyboard::Keycode
};


const FONT_SIZE: u32 = 20;

#[derive(Debug)]
enum RenderValue<'a>
{
    Text{x: i32, y: i32, text: &'a str},
    Line{x: i32, y: i32, width: u32},
    Cursor{x: i32, y: i32}
}

impl RenderValue<'_>
{
    pub fn new_cursor(x: i32, y: i32) -> Self
    {
        Self::Cursor{x, y: y - FONT_SIZE as i32 / 2}
    }

    pub fn new_cursor_rect(rect: RenderRect) -> Self
    {
        Self::new_cursor(rect.x + rect.width as i32, rect.y + rect.height as i32 / 2)
    }

    pub fn shift(&mut self, shift_x: i32, shift_y: i32)
    {
        match self
        {
            Self::Text{x, y, ..} =>
            {
                *x += shift_x;
                *y += shift_y;
            },
            Self::Line{x, y, ..} =>
            {
                *x += shift_x;
                *y += shift_y;
            },
            Self::Cursor{x, y} =>
            {
                *x += shift_x;
                *y += shift_y;
            }
        }
    }
}

#[derive(Debug)]
struct InputValues(Vec<InputValue>);

#[derive(Debug)]
enum InputValue
{
    Value(String),
    Fraction{top: InputValues, bottom: InputValues}
}

impl Default for InputValue
{
    fn default() -> Self
    {
        Self::Value(String::new())
    }
}

impl InputValue
{
    #[allow(dead_code)]
    pub fn is_value(&self) -> bool
    {
        if let Self::Value(_) = self
        {
            true
        } else
        {
            false
        }
    }

    pub fn render(
        &self,
        cursor: Option<&(CursorFollow, Box<ValueCursor>)>,
        x: i32,
        y: i32,
        f: &impl Fn(RenderValue) -> RenderResult
    ) -> RenderResult
    {
        match self
        {
            Self::Value(text) => f(RenderValue::Text{x, y, text}),
            Self::Fraction{top, bottom} =>
            {
                let top_cursor = cursor.and_then(|x@(follow, _)|
                {
                    (*follow == CursorFollow::Top).then_some(&*x.1)
                });

                let mut top = top.render(top_cursor, x, y, f);

                let bottom_cursor = cursor.and_then(|x@(follow, _)|
                {
                    (*follow == CursorFollow::Bottom).then_some(&*x.1)
                });

                let mut bottom = bottom.render(bottom_cursor, x, y, f);

                let (top_shift_x, bottom_shift_x) = if top.rect.width < bottom.rect.width
                {
                    ((bottom.rect.width as i32 - top.rect.width as i32) / 2, 0)
                } else
                {
                    (0, (top.rect.width as i32 - bottom.rect.width as i32) / 2)
                };

                let offset_y = top.rect.height.max(bottom.rect.height) as i32 / 2;
                top.shift(top_shift_x, -offset_y);
                bottom.shift(bottom_shift_x, offset_y);

                let width = top.rect.width.max(bottom.rect.width);

                let line = {
                    let y = (top.rect.y + top.rect.height as i32 + bottom.rect.y) / 2;
                    f(RenderValue::Line{x, y, width})
                };

                let rect = bottom.rect.combine(top.rect);

                let mut render = top.render;
                render.extend(bottom.render);
                render.extend(line.render);

                RenderResult{rect, render}
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RenderRect
{
    x: i32,
    y: i32,
    width: u32,
    height: u32
}

impl From<RenderRect> for Rect
{
    fn from(v: RenderRect) -> Self
    {
        Self::new(v.x, v.y, v.width, v.height)
    }
}

impl From<Rect> for RenderRect
{
    fn from(v: Rect) -> Self
    {
        Self{x: v.x, y: v.y, width: v.width(), height: v.height()}
    }
}

impl RenderRect
{
    pub fn empty() -> Self
    {
        Self{x: 0, y: 0, width: 0, height: 0}
    }

    fn end(self) -> (i32, i32)
    {
        (self.x + self.width as i32, self.y + self.height as i32)
    }

    pub fn combine(self, other: Self) -> Self
    {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);

        let this_end = self.end();
        let other_end = other.end();
        let end_x = this_end.0.max(other_end.0);
        let end_y = this_end.1.max(other_end.1);

        Self{x, y, width: (end_x - x) as u32, height: (end_y - y) as u32}
    }
}

struct RenderResult<'a>
{
    rect: RenderRect,
    render: Vec<RenderValue<'a>>
}

impl<'a> RenderResult<'a>
{
    pub fn new(rect: RenderRect, render: RenderValue<'a>) -> Self
    {
        Self{rect, render: vec![render]}
    }

    pub fn empty(rect: RenderRect) -> Self
    {
        Self{rect, render: Vec::new()}
    }

    fn is_cursor(&self) -> bool
    {
        if self.render.len() == 1
        {
            match &self.render[0]
            {
                RenderValue::Cursor{..} => return true,
                _ => ()
            }
        }

        false
    }

    pub fn combine(mut self, other: RenderResult<'a>) -> Self
    {
        if !other.is_cursor()
        {
            self.rect = self.rect.combine(other.rect);
        }

        self.render.extend(other.render);

        self
    }

    pub fn shift(&mut self, x: i32, y: i32)
    {
        self.rect.x += x;
        self.rect.y += y;

        self.render.iter_mut().for_each(|r| r.shift(x, y));
    }

    pub fn render(&self, renderer: impl FnMut(&RenderValue))
    {
        self.render.iter().for_each(renderer);
    }
}

trait CursorTrait
{
    fn next(self) -> Self;
    fn follow(&self) -> Option<CursorFollow>;
    fn index(&self) -> usize;
}

impl CursorTrait for &ValueCursor
{
    fn next(self) -> Self
    {
        &*self.follow.as_ref().unwrap().1
    }

    fn follow(&self) -> Option<CursorFollow>
    {
        self.follow.as_ref().map(|x| x.0)
    }

    fn index(&self) -> usize
    {
        self.index
    }
}

impl CursorTrait for &mut ValueCursor
{
    fn next(self) -> Self
    {
        &mut *self.follow.as_mut().unwrap().1
    }

    fn follow(&self) -> Option<CursorFollow>
    {
        self.follow.as_ref().map(|x| x.0)
    }

    fn index(&self) -> usize
    {
        self.index
    }
}

macro_rules! define_traverse
{
    ($name:ident, $($ref_t:tt)*) =>
    {
        fn $name<'a, T, C: CursorTrait>(
            &'a $($ref_t)* self,
            cursor: C,
            finish: impl FnOnce(&'a $($ref_t)* Self, C) -> T
        ) -> T
        {
            if let Some(direction) = cursor.follow()
            {
                match (& $($ref_t)* self.0[cursor.index() - 1], direction)
                {
                    (InputValue::Fraction{top, ..}, CursorFollow::Top) =>
                    {
                        top.$name(cursor.next(), finish)
                    },
                    (InputValue::Fraction{bottom, ..}, CursorFollow::Bottom) =>
                    {
                        bottom.$name(cursor.next(), finish)
                    },
                    (InputValue::Value(_), _) => unreachable!()
                }
            } else
            {
                finish(self, cursor)
            }
        }
    }
}

impl Default for InputValues
{
    fn default() -> Self
    {
        Self(Vec::new())
    }
}

impl InputValues
{
    define_traverse!{traverse, }
    define_traverse!{traverse_mut, mut}

    pub fn add_text(&mut self, cursor: &ValueCursor, text: String)
    {
        self.traverse_mut(cursor, |this, cursor| this.0.insert(cursor.index, InputValue::Value(text)));
    }

    pub fn add_fraction(&mut self, cursor: &ValueCursor)
    {
        self.traverse_mut(cursor, |this, cursor|
        {
            if let Some(index) = cursor.index.checked_sub(1)
            {
                let value = mem::take(&mut this.0[index]);

                this.0[index] = InputValue::Fraction{top: Self(vec![value]), bottom: Self(Vec::new())};
            }
        });
    }

    fn replace(&mut self, index: usize, values: InputValues)
    {
        self.0.remove(index);

        values.0.into_iter().rev().for_each(|value|
        {
            self.0.insert(index, value);
        });
    }

    pub fn remove_single(&mut self, cursor: &mut ValueCursor) -> bool
    {
        if let Some((direction, follow)) = cursor.follow.as_mut()
        {
            let index = cursor.index - 1;
            let remove_this = match (&mut self.0[index], direction)
            {
                (InputValue::Fraction{top, bottom}, CursorFollow::Top) =>
                {
                    let remove_this = top.remove_single(follow);

                    if remove_this
                    {
                        let value = mem::take(bottom);
                        self.replace(index, value);
                    }

                    remove_this
                },
                (InputValue::Fraction{top, bottom}, CursorFollow::Bottom) =>
                {
                    let remove_this = bottom.remove_single(follow);

                    if remove_this
                    {
                        let value = mem::take(top);
                        self.replace(index, value);
                    }

                    remove_this
                },
                (InputValue::Value(_), _) => unreachable!()
            };

            if remove_this
            {
                cursor.follow = None;
            }

            false
        } else
        {
            if let Some(index) = cursor.index.checked_sub(1)
            {
                self.0.remove(index);
                cursor.index = index;

                false
            } else
            {
                true
            }
        }
    }

    fn move_right_inner(&self, cursor: &mut ValueCursor) -> bool
    {
        if let Some((direction, follow)) = cursor.follow.as_mut()
        {
            let move_this = match (&self.0[cursor.index - 1], direction)
            {
                (InputValue::Fraction{top, ..}, CursorFollow::Top) =>
                {
                    top.move_right_inner(follow)
                },
                (InputValue::Fraction{bottom, ..}, CursorFollow::Bottom) =>
                {
                    bottom.move_right_inner(follow)
                },
                (InputValue::Value(_), _) => unreachable!()
            };

            if move_this
            {
                cursor.follow = None;
            }

            move_this
        } else
        {
            if cursor.index < self.0.len()
            {
                cursor.index += 1;

                false
            } else
            {
                true
            }
        }
    }

    fn move_left_inner(&self, cursor: &mut ValueCursor) -> bool
    {
        if let Some((_direction, follow)) = cursor.follow.as_mut()
        {
            if self.move_left_inner(follow)
            {
                cursor.follow = None;
                self.move_left_inner(cursor);
            }
        } else
        {
            if let Some(index) = cursor.index.checked_sub(1)
            {
                cursor.index = index;
            } else
            {
                return true;
            }
        }

        false
    }

    fn step_in(&self, cursor: &mut ValueCursor, right: bool) -> bool
    {
        self.traverse(cursor, |this, cursor|
        {
            if let Some(index) = cursor.index.checked_sub(1)
            {
                match &this.0[index]
                {
                    InputValue::Fraction{top, ..} =>
                    {
                        let index = if right { top.0.len() } else { 0 };
                        let new_cursor = ValueCursor{index, ..Default::default()};

                        cursor.follow = Some((CursorFollow::Top, Box::new(new_cursor)));

                        return true;
                    },
                    InputValue::Value(_) => ()
                }
            }

            false
        })
    }

    pub fn move_left(&self, cursor: &mut ValueCursor)
    {
        if !self.step_in(cursor, true)
        {
            self.move_left_inner(cursor);
        }
    }

    pub fn move_right(&self, cursor: &mut ValueCursor)
    {
        if !self.move_right_inner(cursor)
        {
            self.step_in(cursor, false);
        }
    }

    fn move_vertical(
        &self,
        cursor: &mut ValueCursor,
        which: CursorFollow
    ) -> bool
    {
        if let Some((direction, follow)) = cursor.follow.as_mut()
        {
            let this = &self.0[cursor.index - 1];

            if follow.follow.is_none()
            {
                if *direction == which
                {
                    *direction = which.opposite();

                    if let InputValue::Fraction{top, bottom} = this
                    {
                        let (a, b) = if which == CursorFollow::Top
                        {
                            (top.0.len(), bottom.0.len())
                        } else
                        {
                            (bottom.0.len(), top.0.len())
                        };

                        let diff = a as i32 - b as i32;
                        let half_diff = diff / 2;

                        let limit = b as i32;
                        follow.index = (follow.index as i32 - half_diff).clamp(0, limit) as usize;
                    } else
                    {
                        unreachable!()
                    }

                    return true;
                }

                false
            } else
            {
                match (this, direction)
                {
                    (InputValue::Fraction{top, ..}, CursorFollow::Top) =>
                    {
                        top.move_down(&mut **follow)
                    },
                    (InputValue::Fraction{bottom, ..}, CursorFollow::Bottom) =>
                    {
                        bottom.move_down(&mut **follow)
                    },
                    (InputValue::Value(_), _) => unreachable!()
                }
            }
        } else
        {
            false
        }
    }

    pub fn move_up(&self, cursor: &mut ValueCursor) -> bool
    {
        self.move_vertical(cursor, CursorFollow::Bottom)
    }

    pub fn move_down(&self, cursor: &mut ValueCursor) -> bool
    {
        self.move_vertical(cursor, CursorFollow::Top)
    }

    pub fn render(
        &self,
        cursor: Option<&ValueCursor>,
        x: i32,
        y: i32,
        f: &impl Fn(RenderValue) -> RenderResult
    ) -> RenderResult
    {
        let mut start = RenderResult::empty(RenderRect{x, y, width: 0, height: 0});

        if let Some(ValueCursor{index: 0, follow: None}) = cursor
        {
            start = start.combine(f(RenderValue::new_cursor(x, y + FONT_SIZE as i32 / 2)));
        }

        self.0.iter().enumerate().fold(start, |acc, (index, value)|
        {
            let this_index = Some(index + 1) == cursor.map(|x| x.index);
            let cursor = cursor.and_then(|cursor|
            {
                this_index.then(|| { cursor.follow.as_ref() }).flatten()
            });

            let render = value.render(cursor, x + acc.rect.width as i32, y, f);
            let rect = render.rect;

            let mut combined = acc.combine(render);
            if this_index && cursor.is_none()
            {
                combined = combined.combine(f(RenderValue::new_cursor_rect(rect)));
            }

            combined
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CursorFollow
{
    Top,
    Bottom
}

impl CursorFollow
{
    pub fn opposite(self) -> Self
    {
        match self
        {
            Self::Top => Self::Bottom,
            Self::Bottom => Self::Top
        }
    }
}

#[derive(Debug)]
struct ValueCursor
{
    index: usize,
    follow: Option<(CursorFollow, Box<ValueCursor>)>
}

impl Default for ValueCursor
{
    fn default() -> Self
    {
        Self{index: 0, follow: None}
    }
}

impl ValueCursor
{
    pub fn add_fraction(&mut self)
    {
        if let Some((_, follow)) = self.follow.as_mut()
        {
            follow.add_fraction();
        } else
        {
            if self.index != 0
            {
                self.follow = Some((CursorFollow::Bottom, Box::new(Self::default())));
            }
        }
    }

    pub fn added(&mut self)
    {
        if let Some((_direction, follow)) = self.follow.as_mut()
        {
            follow.added();
        } else
        {
            self.index += 1;
        }
    }
}

struct Cursor
{
    line: usize,
    value: ValueCursor
}

struct ProgramState<'a>
{
    font: Font<'a, 'static>,
    cursor: Cursor,
    lines: Vec<InputValues>
}

impl<'a> ProgramState<'a>
{
    pub fn new(font: Font<'a, 'static>) -> Self
    {
        Self{
            font,
            cursor: Cursor{line: 0, value: ValueCursor::default()},
            lines: vec![InputValues::default()]
        }
    }

    pub fn add_text(&mut self, text: String)
    {
        match text.as_ref()
        {
            "/" => self.add_fraction(),
            _ => self.add_normal(text)
        }
    }

    pub fn new_line(&mut self)
    {
        if self.cursor.value.follow.is_some()
        {
            return;
        }

        let rest = self.lines[self.cursor.line].0.split_off(self.cursor.value.index);

        self.cursor.line += 1;
        self.cursor.value = ValueCursor::default();

        self.lines.insert(self.cursor.line, InputValues(rest));
    }

    fn add_normal(&mut self, text: String)
    {
        self.lines[self.cursor.line].add_text(&self.cursor.value, text);
        self.cursor.value.added();
    }

    fn add_fraction(&mut self)
    {
        self.lines[self.cursor.line].add_fraction(&self.cursor.value);
        self.cursor.value.add_fraction();
    }

    pub fn remove_single(&mut self)
    {
        if self.cursor.value.follow.is_none() && self.cursor.value.index == 0
        {
            if self.lines.len() == 1
            {
                return;
            }

            let previous = self.lines.remove(self.cursor.line);

            self.cursor.line -= 1;

            self.cursor.value.follow = None;
            self.cursor.value.index = self.lines[self.cursor.line].0.len();

            self.lines[self.cursor.line].0.extend(previous.0);
        } else
        {
            self.lines[self.cursor.line].remove_single(&mut self.cursor.value);
        }
    }

    pub fn remove_next_single(&mut self)
    {
        let line_length = self.lines[self.cursor.line].0.len();
        if self.cursor.value.follow.is_none() && self.cursor.value.index == line_length
        {
            if self.lines.len() - 1 > self.cursor.line
            {
                let line = self.lines.remove(self.cursor.line + 1);

                self.lines[self.cursor.line].0.extend(line.0);
            }
        } else
        {
            self.move_right();
            self.remove_single();
        }
    }

    pub fn move_left(&mut self)
    {
        self.lines[self.cursor.line].move_left(&mut self.cursor.value);
    }

    pub fn move_right(&mut self)
    {
        self.lines[self.cursor.line].move_right(&mut self.cursor.value);
    }

    fn truncate_index(&mut self)
    {
        self.cursor.value.index = self.cursor.value.index.min(self.lines[self.cursor.line].0.len());
    }

    pub fn move_up(&mut self)
    {
        if !self.lines[self.cursor.line].move_up(&mut self.cursor.value)
        {
            if self.cursor.value.follow.is_none() && self.cursor.line > 0
            {
                self.cursor.line -= 1;
                self.truncate_index();
            }
        }
    }

    pub fn move_down(&mut self)
    {
        if !self.lines[self.cursor.line].move_down(&mut self.cursor.value)
        {
            if self.cursor.value.follow.is_none() && self.cursor.line < self.lines.len() - 1
            {
                self.cursor.line += 1;
                self.truncate_index();
            }
        }
    }

    pub fn render(
        &self,
        width: u32,
        height: u32,
        _highlight: impl FnMut(Rect),
        f: impl Fn(RenderValue) -> RenderResult,
        renderer: impl FnMut(&RenderValue)
    )
    {
        let start = RenderRect::empty();
        let mut render = self.lines.iter().enumerate()
            .fold(RenderResult::empty(start), |acc, (index, line)|
            {
                let cursor = (self.cursor.line == index).then_some(&self.cursor.value);

                let y = acc.rect.y + acc.rect.height as i32;
                let mut rendered = line.render(cursor, 0, y, &f);

                let diff = y - rendered.rect.y;

                rendered.shift(0, diff);

                acc.combine(rendered)
            });

        let center = |size, start, other_size|
        {
            start + (size as i32 - other_size as i32) / 2
        };

        let x = center(width, render.rect.x, render.rect.width);
        let y = center(height, render.rect.y, render.rect.height);

        render.shift(x, y);

        if render.rect.y < 0
        {
            render.shift(0, render.rect.y);
        }

        if render.rect.x < 0
        {
            render.shift(render.rect.x, 0);
        }

        render.render(renderer);
    }
}

fn main()
{
    let ctx = sdl2::init().unwrap();

    let video = ctx.video().unwrap();

    let window = video.window("lil fun algebra thing", 640, 480)
        .resizable()
        .build()
        .unwrap();

    let mut canvas = window.into_canvas().build().unwrap();
    let creator = canvas.texture_creator();

    let mut events = ctx.event_pump().unwrap();

    fn redraw_window(
        state: &ProgramState,
        creator: &TextureCreator<WindowContext>,
        canvas: &mut WindowCanvas
    )
    {
        canvas.set_draw_color(Color::RGB(255, 255, 255));
        canvas.clear();

        let (width, height) = canvas.window().size();

        let canvas = RefCell::new(canvas);

        state.render(width, height, |rect|
        {
            canvas.borrow_mut().set_draw_color(Color::RGB(200, 200, 200));

            canvas.borrow_mut().fill_rect(rect).unwrap();
        }, |render|
        {
            let rect = match render
            {
                RenderValue::Text{x, y, text: value} =>
                {
                    let text = state.font.render(value).blended(Color::RGB(0, 0, 0)).unwrap();

                    Rect::new(x, y, text.width(), text.height())
                },
                RenderValue::Line{x, y, width} =>
                {
                    let height = 2;
                    Rect::new(x, y - height as i32 / 2, width, height)
                },
                RenderValue::Cursor{x, y} =>
                {
                    Rect::new(x, y, 0, 0)
                }
            };

            RenderResult::new(rect.into(), render)
        }, |render|
        {
            canvas.borrow_mut().set_draw_color(Color::RGB(0, 0, 0));

            match render
            {
                RenderValue::Text{x, y, text: value} =>
                {
                    let text = state.font.render(value).blended(Color::RGB(0, 0, 0)).unwrap();
                    let texture = Texture::from_surface(&text, creator).unwrap();

                    let rect = Rect::new(*x, *y, text.width(), text.height());
                    canvas.borrow_mut().copy(&texture, None, rect).unwrap();
                },
                RenderValue::Line{x, y, width} =>
                {
                    let height = 2;
                    let rect = Rect::new(*x, y - height as i32 / 2, *width, height);
                    canvas.borrow_mut().fill_rect(rect).unwrap();
                },
                RenderValue::Cursor{x, y} =>
                {
                    let cursor_height = FONT_SIZE;
                    canvas.borrow_mut().fill_rect(Rect::new(
                        *x,
                        *y,
                        4,
                        cursor_height
                    )).unwrap();
                }
            }
        });

        canvas.borrow_mut().present();
    }

    let ttf_ctx = sdl2::ttf::init().unwrap();
    let font = ttf_ctx.load_font("font/LiberationMono-Regular.ttf", FONT_SIZE as u16).unwrap();

    let mut state = ProgramState::new(font);

    for event in events.wait_iter()
    {
        match event
        {
            Event::Quit{..} => return,
            Event::TextInput{text, ..} =>
            {
                state.add_text(text);
                redraw_window(&state, &creator, &mut canvas);
            },
            Event::KeyDown{keycode: Some(key), ..} =>
            {
                match key
                {
                    Keycode::BACKSPACE =>
                    {
                        state.remove_single();
                    },
                    Keycode::DELETE =>
                    {
                        state.remove_next_single();
                    },
                    Keycode::RETURN =>
                    {
                        state.new_line();
                    },
                    Keycode::LEFT =>
                    {
                        state.move_left();
                    },
                    Keycode::RIGHT =>
                    {
                        state.move_right();
                    },
                    Keycode::UP =>
                    {
                        state.move_up();
                    },
                    Keycode::DOWN =>
                    {
                        state.move_down();
                    },
                    _ => continue
                }

                redraw_window(&state, &creator, &mut canvas);
            },
            Event::Window{win_event, ..} =>
            {
                match win_event
                {
                    WindowEvent::Exposed =>
                    {
                        redraw_window(&state, &creator, &mut canvas);
                    },
                    _ => ()
                }
            },
            _ => ()
        }
    }
}
