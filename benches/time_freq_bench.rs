#![feature(test)]
#![feature(box_syntax)]
extern crate basic_dsp;
extern crate test;

pub mod tools;

#[cfg(test)]
mod time_freq {
    use basic_dsp::window_functions::TriangularWindow;
    use basic_dsp::*;
    use test::Bencher;
    use tools::*;

    #[bench]
    fn plain_fft_ifft_32t_benchmark(b: &mut Bencher) {
        let mut vector = Gen32Box::with_size(2000);
        b.iter(|| {
            vector.execute(|v, buffer| {
                if v.domain() == DataDomain::Time {
                    v.plain_fft(buffer)
                } else {
                    v.plain_ifft(buffer)
                }
            })
        });
    }

    #[bench]
    fn window_32t_benchmark(b: &mut Bencher) {
        let mut vector = Gen32Box::with_size(2000);
        b.iter(|| {
            vector.execute(|mut v, _| {
                let triag = TriangularWindow;
                v.apply_window(&triag);
                v
            })
        });
    }

    #[bench]
    fn fft_ifft_32t_benchmark(b: &mut Bencher) {
        let mut vector = Gen32Box::with_size(2000);
        b.iter(|| {
            vector.execute(|v, buffer| {
                if v.domain() == DataDomain::Time {
                    v.fft(buffer)
                } else {
                    v.ifft(buffer)
                }
            })
        });
    }
}
