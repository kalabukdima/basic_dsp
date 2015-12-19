use multicore_support::{Chunk, Complexity};
use super::definitions::{
	DataVector,
    VecResult,
    VoidResult,
    ErrorReason,
    ScalarResult,
    Statistics,
	ComplexVectorOperations};
use super::GenericDataVector;
use simd_extensions::{Simd,Reg32,Reg64};
use num::complex::Complex;
use num::traits::Float;
use std::ops::Range;
use std::{f32, f64};

macro_rules! add_complex_impl {
    ($($data_type:ident, $reg:ident);*)
	 =>
	 {	 
        $(
            #[inline]
            impl ComplexVectorOperations<$data_type> for GenericDataVector<$data_type>
            {
                type RealPartner = GenericDataVector<$data_type>;
                fn complex_offset(self, offset: Complex<$data_type>)  -> VecResult<Self>
                {
                    assert_complex!(self);
                    let vector_offset = $reg::from_complex(offset);
                    self.simd_complex_operation(|x,y| x + y, |x,y| x + Complex::<$data_type>::new(y.extract(0), y.extract(1)), vector_offset)
                }
                
                fn complex_scale(self, factor: Complex<$data_type>) -> VecResult<Self>
                {
                    assert_complex!(self);
                    self.simd_complex_operation(|x,y| x.scale_complex(y), |x,y| x * y, factor)
                }
                
                fn complex_abs(self) -> VecResult<Self>
                {
                    assert_complex!(self);
                    self.simd_complex_to_real_operation(|x,_arg| x.complex_abs(), |x,_arg| x.norm(), ())
                }
                
                fn get_complex_abs(&self, destination: &mut Self) -> VoidResult
                {
                    if !self.is_complex {
                        return Err(ErrorReason::VectorMustBeComplex);
                    }
                    
                    let data_length = self.len();
                    destination.reallocate(data_length / 2);
                    let scalar_length = data_length % $reg::len();
                    let vectorization_length = data_length - scalar_length;
                    let array = &self.data;
                    let mut temp = &mut destination.data;
                    Chunk::execute_original_to_target(
                        Complexity::Small, &self.multicore_settings,
                        &array, vectorization_length, $reg::len(), 
                        &mut temp, vectorization_length / 2, $reg::len() / 2, 
                        Self::complex_abs_simd);
                    let mut i = vectorization_length;
                    while i + 1 < data_length
                    {
                        temp[i / 2] = (array[i] * array[i] + array[i + 1] * array[i + 1]).sqrt();
                        i += 2;
                    }
                    
                    destination.is_complex = false;
                    destination.delta = self.delta;
                    Ok(())
                }
                
                fn complex_abs_squared(self) -> VecResult<Self>
                {
                    assert_complex!(self);
                    self.simd_complex_to_real_operation(|x,_arg| x.complex_abs_squared(), |x,_arg| x.re * x.re + x.im * x.im, ())
                }
                
                fn complex_conj(self) -> VecResult<Self>
                {
                    assert_complex!(self);
                    let multiplicator = $reg::from_complex(Complex::<$data_type>::new(1.0, -1.0));
                    self.simd_complex_operation(|x,y| x * y, |x,_arg| x * Complex::<$data_type>::new(1.0, -1.0), multiplicator)
                }
                
                fn to_real(mut self) -> VecResult<Self>
                {
                    {
                        assert_complex!(self);
                        let len = self.len();
                        let mut array = temp_mut!(self, len);
                        let source = &self.data;
                        Chunk::execute_original_to_target(
                            Complexity::Small, &self.multicore_settings,
                            &source, len, 2, &mut array, len / 2, 1, 
                            |original, range, target| {
                                let mut i = range.start;
                                let mut j = 0;
                                while j < target.len()
                                { 
                                    target[j] = original[i];
                                    i += 2;
                                    j += 1;
                                }
                        });
                    }
                    
                    self.is_complex = false;
                    self.valid_len = self.valid_len / 2;
                    Ok(self.swap_data_temp())
                }
            
                fn to_imag(mut self) -> VecResult<Self>
                {
                   {
                       assert_complex!(self);
                        let len = self.len();
                        let mut array = temp_mut!(self, len);
                        let source = &self.data;
                        Chunk::execute_original_to_target(
                            Complexity::Small, &self.multicore_settings,
                            &source, len, 2, 
                            &mut array, len / 2, 1, 
                            |original, range, target| {
                                let mut i = range.start + 1;
                                let mut j = 0;
                                while j < target.len()
                                { 
                                    target[j] = original[i];
                                    i += 2;
                                    j += 1;
                                }
                        });
                    }
                    
                    self.is_complex = false;
                    self.valid_len = self.valid_len / 2;
                    Ok(self.swap_data_temp())
                }	
                        
                fn get_real(&self, destination: &mut Self) -> VoidResult
                {
                    if !self.is_complex {
                        return Err(ErrorReason::VectorMustBeComplex);
                    }
                    
                    let len = self.len();
                    destination.reallocate(len / 2);
                    destination.delta = self.delta;
                    destination.is_complex = false;
                    let mut array = &mut destination.data;
                    let source = &self.data;
                    Chunk::execute_original_to_target(
                        Complexity::Small, &self.multicore_settings,
                        &source, len, 2, 
                        &mut array, len / 2, 1, 
                        |original, range, target| {
                            let mut i = range.start;
                            let mut j = 0;
                            while j < target.len()
                            { 
                                target[j] = original[i];
                                i += 2;
                                j += 1;
                            }
                    });
                    
                    Ok(())
                }
                
                fn get_imag(&self, destination: &mut Self) -> VoidResult
                {
                    if !self.is_complex {
                        return Err(ErrorReason::VectorMustBeComplex);
                    }
                    
                    let len = self.len();
                    destination.reallocate(len / 2);
                    destination.delta = self.delta;
                    destination.is_complex = false;
                    let mut array = &mut destination.data;
                    let source = &self.data;
                    Chunk::execute_original_to_target(
                        Complexity::Small, &self.multicore_settings,
                        &source, len, 2, 
                        &mut array, len / 2, 1, 
                        |original, range, target| {
                            let mut i = range.start + 1;
                            let mut j = 0;
                            while j < target.len()
                            { 
                                target[j] = original[i];
                                i += 2;
                                j += 1;
                            }
                    });
                    
                    Ok(())
                }
                
                fn phase(mut self) -> VecResult<Self>
                {
                    {
                        assert_complex!(self);
                        let len = self.len();
                        let mut array = temp_mut!(self, len);
                        let source = &self.data;
                        Chunk::execute_original_to_target(
                            Complexity::Small, &self.multicore_settings,
                            &source, len, 2, 
                            &mut array, len / 2, 1, 
                            Self::phase_par);
                    }
                    
                    self.is_complex = false;
                    self.valid_len = self.valid_len / 2;
                    Ok(self.swap_data_temp())
                }
                
                fn get_phase(&self, destination: &mut Self) -> VoidResult
                {
                    if !self.is_complex {
                        return Err(ErrorReason::VectorMustBeComplex);
                    }
                    
                    let len = self.len();
                    destination.reallocate(len / 2);
                    destination.delta = self.delta;
                    destination.is_complex = false;
                    let mut array = &mut destination.data;
                    let source = &self.data;
                    Chunk::execute_original_to_target(
                        Complexity::Small, &self.multicore_settings,
                        &source, len, 2, 
                        &mut array, len / 2, 1, 
                        Self::phase_par);
                    Ok(())
                }
                
                fn complex_dot_product(&self, factor: &Self) -> ScalarResult<Complex<$data_type>>
                {
                    if !self.is_complex {
                        return Err(ErrorReason::VectorMustBeComplex);
                    }
                    
                    if !factor.is_complex ||
                        self.domain != factor.domain {
                        return Err(ErrorReason::VectorMetaDataMustAgree);
                    }
                    
                    let data_length = self.len();
                    let scalar_length = data_length % $reg::len();
                    let vectorization_length = data_length - scalar_length;
                    let array = &self.data;
                    let other = &factor.data;
                    let chunks = Chunk::get_a_fold_b(
                        Complexity::Small, &self.multicore_settings,
                        &other, vectorization_length, $reg::len(), 
                        &array, vectorization_length, $reg::len(), 
                        |original, range, target| {
                            let mut i = 0;
                            let mut j = range.start;
                            let mut result = $reg::splat(0.0);
                            while i < target.len()
                            { 
                                let vector1 = $reg::load(original, j);
                                let vector2 = $reg::load(target, i);
                                result = result + (vector2.mul_complex(vector1));
                                i += $reg::len();
                                j += $reg::len();
                            }
                        
                        result.sum_complex()        
                    });
                    let mut i = vectorization_length;
                    let mut sum = Complex::<$data_type>::new(0.0, 0.0);
                    while i < data_length
                    {
                        let a = Complex::<$data_type>::new(array[i], array[i + 1]);
                        let b = Complex::<$data_type>::new(other[i], other[i + 1]);
                        sum = sum + a * b;
                        i += 2;
                    }
                    
                    let chunk_sum: Complex<$data_type> = chunks.iter().fold(Complex::<$data_type>::new(0.0, 0.0), |a, b| a + b);
                    Ok(chunk_sum + sum)
                }
                
                fn complex_statistics(&self) -> Statistics<Complex<$data_type>> {
                    let data_length = self.len();
                    let array = &self.data;
                    let chunks = Chunk::get_chunked_results(
                        Complexity::Small, &self.multicore_settings,
                        &array, data_length, 2, 
                        |array, range| {
                            let mut i = 0;
                            let mut sum = Complex::<$data_type>::new(0.0, 0.0);
                            let mut sum_squared = Complex::<$data_type>::new(0.0, 0.0);
                            let mut max = Complex::<$data_type>::new(array[0], array[1]);
                            let mut min = Complex::<$data_type>::new(array[0], array[1]);
                            let mut max_norm = max.norm();
                            let mut min_norm = min.norm();
                            let mut max_index = 0;
                            let mut min_index = 0;
                            while i < array.len()
                            { 
                                let value = Complex::<$data_type>::new(array[i], array[i + 1]);
                                sum = sum + value;
                                sum_squared = sum_squared + value * value;
                                if value.norm() > max_norm {
                                    max = value;
                                    max_index = (i + range.start) / 2;
                                    max_norm = max.norm();
                                }
                                else if value.norm() < min_norm  {
                                    min = value;
                                    min_index = (i + range.start) / 2;
                                    min_norm = min.norm();
                                }
                                
                                i += 2;
                            }
                            
                            Statistics {
                                sum: sum,
                                count: array.len() / 2,
                                average: Complex::<$data_type>::new(0.0, 0.0), 
                                min: min,
                                max: max, 
                                rms: sum_squared, // this field therefore has a different meaning inside this function
                                min_index: min_index,
                                max_index: max_index,
                            }    
                    });
                    
                    Self::merge_complex_stats(&chunks)
                }
                
                fn complex_statistics_splitted(&self, len: usize) -> Vec<Statistics<Complex<$data_type>>> {
                    if len == 0 {
                        return Vec::new();
                    }
                    
                    let data_length = self.len();
                    let array = &self.data;
                    let chunks = Chunk::get_chunked_results_with_arguments (
                        Complexity::Small, &self.multicore_settings,
                        &array, data_length, 1, len,
                        |array, range, len| {
                            let mut i = 0;
                            let mut results = Vec::with_capacity(len);
                            while i < len {
                                let stats = Statistics {
                                        sum: Complex::<$data_type>::new(0.0, 0.0),
                                        count: 0,
                                        average: Complex::<$data_type>::new(0.0, 0.0),
                                        min: Complex::<$data_type>::new($data_type::INFINITY, $data_type::INFINITY),
                                        max: Complex::<$data_type>::new(0.0, 0.0), 
                                        rms: Complex::<$data_type>::new(0.0, 0.0), // this field therefore has a different meaning inside this function
                                        min_index: 0,
                                        max_index: 0,
                                    };
                                results.push(stats);
                                i += 1;
                            }
                            
                            let mut i = 0;
                            while i < array.len()
                            { 
                                let stat = &mut results[(i / 2) % len];
                                let value = Complex::<$data_type>::new(array[i], array[i + 1]);
                                stat.sum = stat.sum + value;
                                stat.rms = stat.rms + value * value;
                                if value.norm() > stat.max.norm() {
                                    stat.max = value;
                                    stat.max_index = (i + range.start) / 2;
                                }
                                else if value.norm() < stat.min.norm()  {
                                    stat.min = value;
                                    stat.min_index = (i + range.start) / 2;
                                }
                                
                                i += 2;
                            }
                            
                            results 
                    });
                    
                    let mut results = Vec::with_capacity(len);
                    print!("{}", chunks.len());
                    print!("{}", chunks[0].len());
                    print!("{:?}", chunks[0][0]);
                    print!("{:?}", chunks[0][1]);
                    for i in 0..len {
                        let mut reordered = Vec::with_capacity(chunks.len());
                        for j in 0..chunks.len()
                        {
                            reordered.push(chunks[j][i]);
                        }
                        
                        let stats = Self::merge_complex_stats(&reordered);
                        results.push(stats);
                    }
                    
                    results
                }
                
                fn get_real_imag(&self, real: &mut Self::RealPartner, imag: &mut Self::RealPartner) -> VoidResult {
                    let data_length = self.len();
                    real.reallocate(data_length / 2);
                    imag.reallocate(data_length / 2);
                    let data = &self.data;
                    for i in 0..data_length {
                        if i % 2 == 0 {
                            real[i / 2] = data[i];
                        } else {
                            imag[i / 2] = data[i];
                        }
                    }
                    
                    Ok(())
                }
                
                fn get_mag_phase(&self, mag: &mut Self::RealPartner, phase: &mut Self::RealPartner) -> VoidResult {
                    let data_length = self.len();
                    mag.reallocate(data_length / 2);
                    phase.reallocate(data_length / 2);
                    let data = &self.data;
                    let mut i = 0;
                    while i < data_length {
                        let c = Complex::<$data_type>::new(data[i], data[i + 1]);
                        let (m, p) = c.to_polar();
                        mag[i / 2] = m;
                        phase[i / 2] = p;
                        i += 2;
                    }
                    
                    Ok(())
                }
                
                fn set_real_imag(mut self, real: &Self::RealPartner, imag: &Self::RealPartner) -> VecResult<Self> {
                    {
                        reject_if!(self, real.len() != imag.len(), ErrorReason::InvalidArgumentLength);
                        self.reallocate(2 * real.len());
                        let data_length = self.len();
                        let data = &mut self.data;
                        for i in 0..data_length {
                            if i % 2 == 0 {
                                data[i] = real[i / 2];
                            } else {
                                data[i] = imag[i / 2];
                            }
                        }
                    }
                    
                    Ok(self)
                }
                
                fn set_mag_phase(mut self, mag: &Self::RealPartner, phase: &Self::RealPartner) -> VecResult<Self> {
                    {
                        reject_if!(self, mag.len() != phase.len(), ErrorReason::InvalidArgumentLength);
                        self.reallocate(2 * mag.len());
                        let data_length = self.len();
                        let data = &mut self.data;
                        let mut i = 0;
                        while i < data_length {
                            let c = Complex::<$data_type>::from_polar(&mag[i / 2], &phase[i / 2]);
                            data[i] = c.re;
                            data[i + 1] = c.im;
                            i += 2;
                        }
                    }
                    
                    Ok(self)
                }
            }
            
            impl GenericDataVector<$data_type> {
                fn complex_abs_simd(original: &[$data_type], range: Range<usize>, target: &mut [$data_type])
                {
                    let mut i = 0;
                    let mut j = range.start;
                    while i < target.len()
                    { 
                        let vector = $reg::load(original, j);
                        let result = vector.complex_abs();
                        result.store_half(target, i);
                        j += $reg::len();
                        i += $reg::len() / 2;
                    }
                }
                
                fn phase_par(original: &[$data_type], range: Range<usize>, target: &mut [$data_type])
                {
                    let mut i = range.start;
                    let mut j = 0;
                    while j < target.len()
                    { 
                        let complex = Complex::<$data_type>::new(original[i], original[i + 1]);
                        target[j] = complex.arg();
                        i += 2;
                        j += 1;
                    }
                }
            }
            
            impl GenericDataVector<$data_type> {
                fn merge_complex_stats(stats: &[Statistics<Complex<$data_type>>]) -> Statistics<Complex<$data_type>> {
                    if stats.len() == 0 {
                        return Statistics {
                            sum: Complex::<$data_type>::new(0.0, 0.0),
                            count: 0,
                            average: Complex::<$data_type>::new($data_type::NAN, $data_type::NAN),
                            min: Complex::<$data_type>::new($data_type::NAN, $data_type::NAN),
                            max: Complex::<$data_type>::new($data_type::NAN, $data_type::NAN),
                            rms: Complex::<$data_type>::new($data_type::NAN, $data_type::NAN),
                            min_index: 0,
                            max_index: 0,
                        };
                    }
                    
                    let mut sum = Complex::<$data_type>::new(0.0, 0.0);
                    let mut max = stats[0].max;
                    let mut min = stats[0].min;
                    let mut count = 0;
                    let mut max_index = stats[0].max_index;
                    let mut min_index = stats[0].min_index;
                    let mut max_norm = max.norm();
                    let mut min_norm = min.norm();
                    let mut sum_squared = Complex::<$data_type>::new(0.0, 0.0);
                    for stat in stats {
                        sum = sum + stat.sum;
                        count = count + stat.count;
                        sum_squared = sum_squared + stat.rms; // We stored sum_squared in the field rms
                        if stat.max.norm() > max_norm {
                            max = stat.max;
                            max_norm = max.norm();
                            max_index = stat.max_index;
                        }
                        else if stat.min.norm() > min_norm {
                            min = stat.min;
                            min_norm = min.norm();
                            min_index = stat.min_index;
                        }
                    }
                    
                    Statistics {
                        sum: sum,
                        count: count,
                        average: sum / (count as $data_type),
                        min: min,
                        max: max,
                        rms: (sum_squared / (count as $data_type)).sqrt(),
                        min_index: min_index,
                        max_index: max_index,
                    }  
                }
            }
        )*
     }
}
add_complex_impl!(f32, Reg32; f64, Reg64);