use crate::chart::*;
use geometry::{Cell, DefaultMap, Map, Terrain};
use macroquad::color::{GRAY, YELLOW};
use macroquad::prelude::{
    BLACK, BLUE, Color, GREEN, RED, WHITE, draw_circle, draw_hexagon, draw_line, draw_poly_lines, draw_text,
    next_frame, screen_height, screen_width,
};
use rand::random;
use shared::Params;
#[macroquad::main("Cartographer")]
pub async fn main() {
    let seed = random();
    let params = Params::default();
    let generation_result = Chart::generate(seed, &params);
    let chart;
    match generation_result {
        Ok(returned_chart) => {
            chart = returned_chart;
        }
        Err(GenError::Error(message)) => {
            panic!("{}", message);
        }
    }

    let map: &DefaultMap = chart.map();
    let rings = params.archipelago_radius + 1;

    for cell in (Cell { q: 0, r: 0 }).disc(map.radius()) {
        let terrain = map.terrain(cell).unwrap();

        println!("Cell ({:2}, {:2}) -> {:?}", cell.q, cell.r, terrain);
    }

    loop {
        let cx = screen_width() / 2.0;
        let cy = screen_height() / 2.0;
        let r = (screen_height().min(screen_width())) / 2.0 - 1.0;

        //draw_circle(cx, cy, 1.0, WHITE);
        //draw_text(screen_width().to_string(), cx, screen_height() - 10.0, 20.0, WHITE);
        //draw_text(screen_height().to_string(), 10.0, cy, 20.0, WHITE);

        //draw_line(cx, cy, cx, cy + 3.0f32.sqrt() * r / 2.0, 1.0, GREEN);
        //draw_line(cx, cy, cx + r, cy, 1.0, GREEN);

        //draw_poly_lines(cx, cy, 255, r, 0., 1.0, BLUE);
        //draw_poly_lines(cx, cy, 255, 3.0f32.sqrt()/2.0 * r, 0., 1.0, RED);

        draw_hexagon(cx, cy, r, 1.0, false, GREEN, Color::new(0.0, 0.0, 0.0, 0.0));

        let hr = r / rings as f32 / 2.0;
        let hw = hr * 2.0;
        let hh = 3.0f32.sqrt() * hr;
        draw_line(cx - hh / 2.0, cy, cx + hh / 2.0, cy, 1.0, WHITE);
        draw_line(cx, cy - hw / 2.0, cx, cy + hw / 2.0, 1.0, WHITE);
        draw_hexagon(cx, cy, hr, 1.0, true, BLUE, Color::new(0.0, 0.0, 0.0, 0.0));

        for cell in (Cell { q: 0, r: 0 }).disc(map.radius()) {
            let terrain = map.terrain(cell).unwrap();
            let (hx, hy) = coords_to_pos((cell.q, cell.r), hw, hh);
            draw_hexagon(cx + hx, cy + hy, hr, 1.0, true, WHITE, terrain_to_color(terrain));
            //draw_text(format!("{} {}", cell.q, cell.r), cx + hx - 15.0, cy + hy, 20.0, WHITE);
        }
        next_frame().await;
    }

    fn coords_to_pos(coords: (i32, i32), hw: f32, hh: f32) -> (f32, f32) {
        let q = coords.0 as f32;
        let r = coords.1 as f32;
        (hw * (q + r / 2.0), hh * (r))
    }

    fn terrain_to_color(t: Terrain) -> Color {
        match t {
            Terrain::Clear => BLUE,
            Terrain::Cliff => GRAY,
            Terrain::Island(a) => YELLOW,
            Terrain::Rock => BLACK,
        }
    }
}
