#[derive(Default)]
pub struct Biquad {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,

    pub x1: f64,
    pub x2: f64,
    pub y1: f64,
    pub y2: f64,
}

impl Biquad {
    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    pub fn print(&self) {
        eprintln!("b0: {}\nb1: {}\nb2: {}\na1: {}\na2: {}",
                  self.b0, self.b1, self.b2, self.a1, self.a2);
    }
}

pub fn iir(input: &[f64], output: &mut [f64], bq: &mut Biquad) {
    if input.len() != output.len() {
        return;
    }

    for i in 0..input.len() {
        let x = input[i];
        let y = (bq.b0 * x) + (bq.b1 * bq.x1) + (bq.b2 * bq.x2) + (bq.a1 * bq.y1) + (bq.a2 * bq.y2);

        output[i] = y;

        bq.x2 = bq.x1;
        bq.x1 = x;

        bq.y2 = bq.y1;
        bq.y1 = y;
    }
}
