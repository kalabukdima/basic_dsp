use RealNumber;
use conv_types::{
    RealImpulseResponse,
    RealFrequencyResponse};
use num::traits::Zero;
use std::ops::{Add, Mul};
use super::WrappingIterator;
use simd_extensions::*;
use super::super::{
	array_to_complex, array_to_complex_mut,
	VoidResult, DspVec, Domain, NumberSpace, ToSliceMut,
	GenDspVec, ToDspVector, DataDomain, Buffer, Vector
};
use std::mem;

/// Provides interpolation operations for real and complex data vectors.
/// # Unstable
/// This functionality has been recently added in order to find out if the definitions are consistent.
/// However the actual implementation is lacking tests.
pub trait InterpolationOps<S, T>
    where S: ToSliceMut<T>,
		T : RealNumber {
    /// Interpolates `self` with the convolution function `function` by the real value `interpolation_factor`.
    /// InterpolationOps is done in time domain and the argument `conv_len` can be used to balance accuracy
    /// and computational performance.
    /// A `delay` can be used to delay or phase shift the vector. The `delay` considers `self.delta()`.
    ///
    /// The complexity of this `interpolatef` is `O(self.points() * conv_len)`, while for `interpolatei` it's
    /// `O(self.points() * log(self.points()))`. If computational performance is important you should therefore decide
    /// how large `conv_len` needs to be to yield the desired accuracy. If you compare `conv_len` to `log(self.points)` you should
    /// get a feeling for the expected performance difference. More important is however to do a test
    /// run to compare the speed of `interpolatef` and `interpolatei`. Together with the information that
    /// changing the vectors size change `log(self.points()` but not `conv_len` gives the indication that `interpolatef`
    /// performs faster for larger vectors while `interpolatei` performs faster for smaller vectors.
    fn interpolatef<B>(&mut self, buffer: &mut B, function: &RealImpulseResponse<T>, interpolation_factor: T, delay: T, conv_len: usize)
		where B: Buffer<S, T>;

    /// Interpolates `self` with the convolution function `function` by the interger value `interpolation_factor`.
    /// InterpolationOps is done in in frequency domain.
    ///
    /// See the description of `interpolatef` for some basic performance considerations.
    /// # Failures
    /// TransRes may report the following `ErrorReason` members:
    ///
    /// 1. `ArgumentFunctionMustBeSymmetric`: if `!self.is_complex() && !function.is_symmetric()` or in words if `self` is a real
    ///    vector and `function` is asymmetric. Converting the vector into a complex vector before the interpolation is one way
    ///    to resolve this error.
    fn interpolatei<B>(&mut self, buffer: &mut B, function: &RealFrequencyResponse<T>, interpolation_factor: u32) -> VoidResult
		where B: Buffer<S, T>;

    /// Decimates or downsamples `self`. `decimatei` is the inverse function to `interpolatei`.
    fn decimatei<B>(&mut self, buffer: &mut B, decimation_factor: u32, delay: u32)
		where B: Buffer<S, T>;
}

/// Provides interpolation operations which are only applicable for real data vectors.
/// # Failures
/// All operations in this trait fail with `VectorMustBeReal` if the vector isn't in the real number space.
pub trait RealInterpolationOps<S, T>
    where S: ToSliceMut<T>,
	T: RealNumber {

    /// Piecewise cubic hermite interpolation between samples.
    /// # Unstable
    /// Algorithm might need to be revised.
    /// This operation and `interpolate_lin` might be merged into one function with an additional argument in future.
    fn interpolate_hermite<B>(&mut self, buffer: &mut B, interpolation_factor: T, delay: T)
		where B: Buffer<S, T>;

    /// Linear interpolation between samples.
    /// # Unstable
    /// This operation and `interpolate_hermite` might be merged into one function with an additional argument in future.
    fn interpolate_lin<B>(&mut self, buffer: &mut B, interpolation_factor: T, delay: T)
		where B: Buffer<S, T>;
}

impl<S, T, N, D> DspVec<S, T, N, D>
	where S: ToSliceMut<T>,
		  T: RealNumber,
		  N: NumberSpace,
		  D: Domain {
	  fn interpolate_priv_scalar<TT> (
		  temp: &mut [TT], data: &[TT],
		  function: &RealImpulseResponse<T>,
		  interpolation_factor: T, delay: T,
		  conv_len: usize)
			  where TT: Zero + Mul<Output=TT> + Copy + Send + Sync + From<T> {
		  let mut i = 0;
		  for num in temp {
			  let center = T::from(i).unwrap() / interpolation_factor;
			  let rounded = center.floor();
			  let iter = WrappingIterator::new(&data, rounded.to_isize().unwrap() - conv_len as isize -1, 2 * conv_len + 1);
			  let mut sum = TT::zero();
			  let mut j = -T::from(conv_len).unwrap() - (center - rounded) + delay;
			  for c in iter {
				  sum = sum + c * TT::from(function.calc(j));
				  j = j + T::one();
			  }
			  (*num) = sum;
			  i += 1;
		  }
	  }

	  fn function_to_vectors(
		  function: &RealImpulseResponse<T>,
		  conv_len: usize,
		  complex_result: bool,
		  interpolation_factor: usize,
		  delay: T) -> Vec<GenDspVec<Vec<T>, T>> {
		  let mut result = Vec::with_capacity(interpolation_factor);
		  for shift in 0..interpolation_factor {
			  let offset = T::from(shift).unwrap() / T::from(interpolation_factor).unwrap();
			  result.push(Self::function_to_vector(
				  function,
				  conv_len,
				  complex_result,
				  offset,
				  delay));
		  }

		  result
	  }

	  fn function_to_vector(
		  function: &RealImpulseResponse<T>,
		  conv_len: usize,
		  complex_result: bool,
		  offset: T,
		  delay: T) -> GenDspVec<Vec<T>, T> {
		  let step = if complex_result { 2 } else { 1 };
		  let data_len = step * (2 * conv_len + 1);
		  let mut imp_resp =
		  	vec!(T::zero(); data_len).to_gen_dsp_vec(complex_result, DataDomain::Time);
		  let mut i = 0;
		  let mut j = -(T::from(conv_len).unwrap() - T::one()) + delay;
		  while i < data_len {
			  let value = function.calc(j - offset);
			  imp_resp[i] = value;
			  i += step;
			  j = j + T::one();
		  }
		  imp_resp
	  }

	  fn interpolate_priv_simd<TT, C, CMut, RMul, RSum, B>(
		  &mut self,
		  buffer: &mut B,
		  function: &RealImpulseResponse<T>,
		  interpolation_factor: usize,
		  delay: T,
		  conv_len: usize,
		  new_len: usize,
		  convert: C,
		  convert_mut: CMut,
		  simd_mul: RMul,
		  simd_sum: RSum)
			  where
				  TT: Zero + Clone + From<T> + Copy + Add<Output=TT> + Mul<Output=TT>,
				  C: Fn(&[T]) -> &[TT],
				  CMut: Fn(&mut [T]) -> &mut [TT],
				  RMul: Fn(T::Reg, T::Reg) -> T::Reg,
				  RSum: Fn(T::Reg) -> TT,
				  B: Buffer<S, T> {
		  let len = self.len();
		  let mut temp = buffer.get(new_len);
		  {
			  let step = if self.is_complex() { 2 } else { 1 };
			  let number_of_shifts = T::Reg::len() / step;
			  let vectors = Self::function_to_vectors(
				  function,
				  conv_len,
				  self.is_complex(),
				  interpolation_factor,
				  delay);
			  let mut shifts = Vec::with_capacity(vectors.len() * number_of_shifts);
			  for vector in &vectors {
				  let shifted_copies = DspVec::create_shifted_copies(&vector);
				  for shift in shifted_copies {
					  shifts.push(shift);
				  }
			  }

			  let data = self.data.to_slice();
			  let mut temp = temp.to_slice_mut();
			  let dest = convert_mut(&mut temp[0..new_len]);
              let len = dest.len();
			  let scalar_len = vectors[0].points() * interpolation_factor;
			  let mut i = 0;
			  {
				  let data = convert(&data[0..len]);
				  for num in &mut dest[0..scalar_len] {
					  (*num) =
						  Self::interpolate_priv_simd_step(
							  i, interpolation_factor, conv_len,
							  data, &vectors);
					  i += 1;
				  }
			  }

			  let (scalar_left, _, vectorization_length) = T::Reg::calc_data_alignment_reqs(&data[0..len]);
			  let simd = T::Reg::array_to_regs(&data[scalar_left..vectorization_length]);
			  // Length of a SIMD reg relative to the length of type T
			  // which is 1 for real numbers or 2 for complex numbers
			  let simd_len_in_t = T::Reg::len() / step;
			  for num in &mut dest[scalar_len .. len - scalar_len] {
				  let rounded = (i + interpolation_factor - 1) / interpolation_factor;
				  let end = (rounded + conv_len) as usize;
				  let simd_end = (end + simd_len_in_t - 1) / simd_len_in_t;
				  let simd_shift = end % simd_len_in_t;
				  let factor_shift = i % interpolation_factor;
				  // The reasoning for the next match is analog to the explanation in the
				  // create_shifted_copies function.
				  // We need the inverse of the mod unless we start with zero
				  let factor_shift = match factor_shift {
					  0 => 0,
					  x => interpolation_factor - x
				  };
				  let selection = factor_shift * simd_len_in_t + simd_shift;
				  let shifted = &shifts[selection];
				  let mut sum = T::Reg::splat(T::zero());
				  let simd_iter = simd[simd_end - shifted.len() .. simd_end].iter();
				  let iteration =
					  simd_iter
					  .zip(shifted);
				  for (this, other) in iteration {
					  sum = sum + simd_mul(*this, *other);
				  }
				  (*num) = simd_sum(sum);
				  i += 1;
			  }

			  i = len - scalar_len;
			  {
				  let data = convert(&data[0..len]);
				  for num in &mut dest[len-scalar_len..len] {
					  (*num) =
						  Self::interpolate_priv_simd_step(
							  i, interpolation_factor, conv_len,
							  data, &vectors);
					  i += 1;
				  }
			  }
		  }
		  self.valid_len = new_len;
		  mem::swap(&mut temp, &mut self.data);
		  buffer.free(temp);
	  }

	  #[inline]
	  fn interpolate_priv_simd_step<TT>(
		  i: usize,
		  interpolation_factor: usize,
		  conv_len: usize,
		  data: &[TT],
		  vectors: &Vec<GenDspVec<Vec<T>, T>>) -> TT
			  where
				  TT: Zero + Clone + From<T> + Copy + Add<Output=TT> + Mul<Output=TT> {
		  let rounded = i / interpolation_factor;
		  let iter = WrappingIterator::new(&data, rounded as isize - conv_len as isize, 2 * conv_len + 1);
		  let vector = &vectors[i % interpolation_factor];
		  let step = if vector.is_complex() { 2 } else { 1 };
		  let mut sum = TT::zero();
		  let mut j = 0;
		  for c in iter {
			  sum = sum + c * TT::from(vector[j]);
			  j += step;
		  }
		  sum
	  }
}

impl<S, T, N, D> InterpolationOps<S, T> for DspVec<S, T, N, D>
	where S: ToSliceMut<T>,
		  T: RealNumber,
		  N: NumberSpace,
		  D: Domain {
  fn interpolatef<B>(&mut self, buffer: &mut B, function: &RealImpulseResponse<T>, interpolation_factor: T, delay: T, conv_len: usize)
  	where B: Buffer<S, T> {

	let delay = delay / self.delta;
  	let len = self.len();
  	let points_half = self.points() / 2;
  	let conv_len =
  		if conv_len > points_half {
  			points_half
  		} else {
  			conv_len
  		};
  	let is_complex = self.is_complex();
  	let new_len = (T::from(len).unwrap() * interpolation_factor).round().to_usize().unwrap();
  	let new_len = new_len + new_len % 2;
  	if conv_len <= 202 && new_len >= 2000 &&
  		(interpolation_factor.round() - interpolation_factor).abs() < T::from(1e-6).unwrap() {
  		let interpolation_factor = interpolation_factor.round().to_usize().unwrap();
  		if self.is_complex() {
  			return self.interpolate_priv_simd(
				buffer,
  				function,
  				interpolation_factor,
  				delay,
  				conv_len,
  				new_len,
  				|x| array_to_complex(x),
  				|x| array_to_complex_mut(x),
  				|x,y| x.mul_complex(y),
  				|x| x.sum_complex())
  		} else {
  			return self.interpolate_priv_simd(
				buffer,
  				function,
  				interpolation_factor,
  				delay,
  				conv_len,
  				new_len,
  				|x| x,
  				|x| x,
  				|x,y| x * y,
  				|x| x.sum_real())
  		}
  	}
  	else if is_complex {
		let mut temp = buffer.get(new_len);
		{
			let data = self.data.to_slice();
	  		let data = &data[0..len];
	  		let temp = temp.to_slice_mut();;
	  		let mut temp = array_to_complex_mut(&mut temp[0..new_len]);
	  		let data = array_to_complex(data);
	  		Self::interpolate_priv_scalar(
	  			temp, data,
	  			function,
	  			interpolation_factor, delay, conv_len);
		}
		mem::swap(&mut temp, &mut self.data);
		buffer.free(temp);
  	}
  	else {
		let mut temp = buffer.get(new_len);
		{
			let data = self.data.to_slice();
	  		let data = &data[0..new_len];
			let temp = temp.to_slice_mut();
	  		Self::interpolate_priv_scalar(
	  			temp, data,
	  			function,
	  			interpolation_factor, delay, conv_len);
		}
		mem::swap(&mut temp, &mut self.data);
		buffer.free(temp);
  	}

  	self.valid_len = new_len;
  }

  fn interpolatei<B>(&mut self, buffer: &mut B, function: &RealFrequencyResponse<T>, interpolation_factor: u32) -> VoidResult
  	where B: Buffer<S, T> {
	  panic!("Panic")
  }

  fn decimatei<B>(&mut self, buffer: &mut B, decimation_factor: u32, delay: u32)
  	where B: Buffer<S, T> {
	  panic!("Panic")
  }
}

#[cfg(test)]
mod tests {
    use num::complex::Complex32;
    use conv_types::*;
	use super::super::super::*;
    use RealNumber;

    fn assert_eq_tol<T>(left: &[T], right: &[T], tol: T)
        where T: RealNumber {
        assert_eq!(left.len(), right.len());
        for i in 0..left.len() {
            if (left[i] - right[i]).abs() > tol {
                panic!("assertion failed: {:?} != {:?} at index {}", left, right, i);
            }
        }
    }
/*
    #[test]
    fn interpolatei_sinc_test() {
        let len = 6;
        let mut time = ComplexTimeVector32::from_constant(Complex32::new(0.0, 0.0), len);
        time[len] = 1.0;
        let sinc: SincFunction<f32> = SincFunction::new();
        let result = time.interpolatei(&sinc as &RealFrequencyResponse<f32>, 2).unwrap();
        let result = result.magnitude().unwrap();
        let expected =
            [0.16666667, 0.044658206, 0.16666667, 0.16666667, 0.16666667, 0.6220085,
             1.1666667, 0.6220085, 0.16666667, 0.16666667, 0.16666667, 0.044658206];
        assert_eq_tol(result.real(0..), &expected, 1e-4);
    }

    #[test]
    fn interpolatei_rc_test() {
        let len = 6;
        let mut time = ComplexTimeVector32::from_constant(Complex32::new(0.0, 0.0), len);
        time[len] = 1.0;
        let rc: RaisedCosineFunction<f32> = RaisedCosineFunction::new(0.4);
        let result = time.interpolatei(&rc as &RealFrequencyResponse<f32>, 2).unwrap();
        let result = result.magnitude().unwrap();
        let expected =
            [0.0, 0.038979173, 0.0000000062572014, 0.15530863, 0.000000015884869, 0.6163295,
             1.0, 0.61632943, 0.0000000142918966, 0.15530863, 0.000000048099658, 0.038979173];
        assert_eq_tol(result.real(0..), &expected, 1e-4);
    }*/

    #[test]
    fn interpolatef_sinc_test() {
        let len = 6;
        let mut time = vec!(0.0; 2 * len).to_complex_time_vec();
        time[len] = 1.0;
        let sinc: SincFunction<f32> = SincFunction::new();
		let mut buffer = SingleBuffer::new();
        time.interpolatef(&mut buffer, &sinc as &RealImpulseResponse<f32>, 2.0, 0.0, len);
        let result = time.magnitude();
        let expected =
            [0.00000, 0.04466, 0.00000, 0.16667, 0.00000, 0.62201,
             1.00000, 0.62201, 0.00000, 0.16667, 0.00000, 0.04466];
        assert_eq_tol(&result[..], &expected, 0.1);
    }

    #[test]
    fn interpolatef_delayed_sinc_test() {
        let len = 6;
        let mut time = vec!(0.0; 2 * len).to_complex_time_vec();
        time[len] = 1.0;
        let sinc: SincFunction<f32> = SincFunction::new();
		let mut buffer = SingleBuffer::new();
        time.interpolatef(&mut buffer, &sinc as &RealImpulseResponse<f32>, 2.0, 1.0, len);
        let result = time.magnitude();
        let expected =
            [0.00000, 0.00000, 0.00000, 0.04466, 0.00000, 0.16667,
             0.00000, 0.62201, 1.00000, 0.62201, 0.00000, 0.16667];
        assert_eq_tol(&result[..], &expected, 0.1);
    }
/*
    #[test]
    fn decimatei_test() {
        let time = ComplexTimeVector32::from_interleaved(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0]);
        let result = time.decimatei(2, 1).unwrap();
        let expected = [2.0, 3.0, 6.0, 7.0, 10.0, 11.0];
        assert_eq_tol(result.interleaved(0..), &expected, 0.1);
    }

    #[test]
    fn hermit_spline_test() {
        let time = RealFreqVector32::from_array(&[-1.0, -2.0, -1.0, 0.0, 1.0, 3.0, 4.0]);
        let result = time.interpolate_hermite(4.0, 0.0).unwrap();
        let expected = [
            -1.0000, -1.4375, -1.7500, -1.9375, -2.0000, -1.8906, -1.6250, -1.2969,
            -1.0000, -0.7500, -0.5000, -0.2500, 0.0, 0.2344, 0.4583, 0.7031,
            1.0000, 1.4375, 2.0000, 2.5625, 3.0000, 3.3203, 3.6042, 3.8359, 4.0];
        assert_eq_tol(
            &result.real(0..)[4..expected.len()-4],
            &expected[4..expected.len()-4],
            6e-2);
    }

    #[test]
    fn hermit_spline_test_linear_increment() {
        let time = RealFreqVector32::from_array(&[-3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0]);
        let result = time.interpolate_hermite(3.0, 0.0).unwrap();
        let expected = [
            -3.0, -2.666, -2.333, -2.0, -1.666, -1.333, -1.0, -0.666, -0.333, 0.0,
            0.333, 0.666, 1.0, 1.333, 1.666, 2.0, 2.333, 2.666, 3.0];
        assert_eq_tol(result.real(0..), &expected, 5e-3);
    }

    #[test]
    fn linear_test() {
        let time = RealFreqVector32::from_array(&[-1.0, -2.0, -1.0, 0.0, 1.0, 3.0, 4.0]);
        let result = time.interpolate_lin(4.0, 0.0).unwrap();
        let expected = [
            -1.0000, -1.2500, -1.5000, -1.7500, -2.0000, -1.7500, -1.5000, -1.2500,
            -1.0000, -0.7500, -0.5000, -0.2500, 0.0, 0.2500, 0.5000, 0.7500,
             1.0000, 1.5000, 2.0000, 2.5000, 3.0000, 3.2500, 3.5000, 3.7500, 4.0];
        assert_eq_tol(result.real(0..), &expected, 0.1);
    }*/
}
