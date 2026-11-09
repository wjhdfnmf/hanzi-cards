//! The Whisper network (OpenAI's architecture, the same weights candle's Whisper uses), with
//! a key/value cache for the decoder's self-attention.
//!
//! candle's own decoder recomputes every earlier token for each new one, so a prompt of a
//! hundred tokens makes every step a hundred times the work. Here the prompt is processed
//! once and each new token only attends to the cached keys and values. The audio side
//! (cross-attention keys and values) is also computed once per recording.

use candle_core::{IndexOp, Module, Result, Tensor};
use candle_nn::{linear, linear_no_bias, Conv1d, Conv1dConfig, Embedding, LayerNorm, Linear, VarBuilder};
use candle_transformers::models::whisper::Config;

fn layer_norm(size: usize, vb: VarBuilder) -> Result<LayerNorm> {
    Ok(LayerNorm::new(vb.get(size, "weight")?, vb.get(size, "bias")?, 1e-5))
}

struct Attention {
    q: Linear,
    k: Linear,
    v: Linear,
    out: Linear,
    heads: usize,
}

impl Attention {
    fn load(n: usize, heads: usize, vb: VarBuilder) -> Result<Self> {
        Ok(Attention {
            q: linear(n, n, vb.pp("q_proj"))?,
            k: linear_no_bias(n, n, vb.pp("k_proj"))?,
            v: linear(n, n, vb.pp("v_proj"))?,
            out: linear(n, n, vb.pp("out_proj"))?,
            heads,
        })
    }

    /// (b, t, d) -> (b, heads, t, d / heads), contiguous, optionally scaled.
    fn heads(&self, x: &Tensor, scaled: bool) -> Result<Tensor> {
        let (b, t, d) = x.dims3()?;
        let hd = d / self.heads;
        let x = x.reshape((b, t, self.heads, hd))?.transpose(1, 2)?.contiguous()?;
        if scaled {
            x * (hd as f64).powf(-0.25)
        } else {
            Ok(x)
        }
    }

    /// Keys (scaled) and values in head layout, ready for `attend_heads`.
    fn kv(&self, x: &Tensor) -> Result<(Tensor, Tensor)> {
        Ok((self.heads(&self.k.forward(x)?, true)?, self.heads(&self.v.forward(x)?, false)?))
    }

    /// q: (b, tq, d); k, v: (b, tk, d); mask: (tq, tk) added to the scores.
    fn attend(&self, q: &Tensor, k: &Tensor, v: &Tensor, mask: Option<&Tensor>) -> Result<Tensor> {
        let k = self.heads(k, true)?;
        let v = self.heads(v, false)?;
        self.attend_heads(q, &k, &v, mask)
    }

    /// Like `attend`, with keys/values already in head layout (b, heads, tk, hd).
    fn attend_heads(&self, q: &Tensor, k: &Tensor, v: &Tensor, mask: Option<&Tensor>) -> Result<Tensor> {
        let (b, tq, d) = q.dims3()?;
        let q = self.heads(q, true)?;
        let mut w = q.matmul(&k.transpose(2, 3)?)?;
        if let Some(mask) = mask {
            w = w.broadcast_add(mask)?;
        }
        let w = candle_nn::ops::softmax_last_dim(&w)?;
        let y = w.matmul(v)?.transpose(1, 2)?.contiguous()?.reshape((b, tq, d))?;
        self.out.forward(&y)
    }
}

struct Block {
    attn: Attention,
    attn_ln: LayerNorm,
    cross: Option<(Attention, LayerNorm)>,
    fc1: Linear,
    fc2: Linear,
    mlp_ln: LayerNorm,
}

impl Block {
    fn load(n: usize, heads: usize, cross: bool, vb: VarBuilder) -> Result<Self> {
        Ok(Block {
            attn: Attention::load(n, heads, vb.pp("self_attn"))?,
            attn_ln: layer_norm(n, vb.pp("self_attn_layer_norm"))?,
            cross: if cross {
                Some((Attention::load(n, heads, vb.pp("encoder_attn"))?, layer_norm(n, vb.pp("encoder_attn_layer_norm"))?))
            } else {
                None
            },
            fc1: linear(n, n * 4, vb.pp("fc1"))?,
            fc2: linear(n * 4, n, vb.pp("fc2"))?,
            mlp_ln: layer_norm(n, vb.pp("final_layer_norm"))?,
        })
    }

    fn mlp(&self, x: &Tensor) -> Result<Tensor> {
        x + self.fc2.forward(&self.fc1.forward(&self.mlp_ln.forward(x)?)?.gelu()?)?
    }
}

fn sinusoids(length: usize, channels: usize, dev: &candle_core::Device) -> Result<Tensor> {
    let inc = 10000f32.ln() / (channels / 2 - 1) as f32;
    let inv: Vec<f32> = (0..channels / 2).map(|i| (i as f32 * -inc).exp()).collect();
    let inv = Tensor::new(inv.as_slice(), dev)?.unsqueeze(0)?;
    let t = Tensor::arange(0, length as u32, dev)?.to_dtype(candle_core::DType::F32)?.unsqueeze(1)?;
    let sh = (length, channels / 2);
    let st = (t.broadcast_as(sh)? * inv.broadcast_as(sh)?)?;
    Tensor::cat(&[st.sin()?, st.cos()?], 1)
}

pub struct Encoder {
    conv1: Conv1d,
    conv2: Conv1d,
    pos: Tensor,
    blocks: Vec<Block>,
    ln_post: LayerNorm,
}

impl Encoder {
    pub fn load(vb: VarBuilder, cfg: &Config) -> Result<Self> {
        let n = cfg.d_model;
        let conv = |inp: usize, stride: usize, vb: VarBuilder| -> Result<Conv1d> {
            let c = Conv1dConfig { padding: 1, stride, ..Default::default() };
            Ok(Conv1d::new(vb.get((n, inp, 3), "weight")?, Some(vb.get(n, "bias")?), c))
        };
        Ok(Encoder {
            conv1: conv(cfg.num_mel_bins, 1, vb.pp("conv1"))?,
            conv2: conv(n, 2, vb.pp("conv2"))?,
            pos: sinusoids(cfg.max_source_positions, n, vb.device())?.to_dtype(vb.dtype())?,
            blocks: (0..cfg.encoder_layers)
                .map(|i| Block::load(n, cfg.encoder_attention_heads, false, vb.pp(format!("layers.{i}"))))
                .collect::<Result<_>>()?,
            ln_post: layer_norm(n, vb.pp("layer_norm"))?,
        })
    }

    /// mel: (1, n_mels, frames) -> audio features (1, frames / 2, d)
    pub fn forward(&self, mel: &Tensor) -> Result<Tensor> {
        let x = self.conv1.forward(mel)?.gelu()?;
        let x = self.conv2.forward(&x)?.gelu()?.transpose(1, 2)?;
        let len = x.dim(1)?;
        let mut x = x.broadcast_add(&self.pos.narrow(0, 0, len)?)?;
        for b in &self.blocks {
            let h = b.attn_ln.forward(&x)?;
            let q = b.attn.q.forward(&h)?;
            let k = b.attn.k.forward(&h)?;
            let v = b.attn.v.forward(&h)?;
            x = (&x + b.attn.attend(&q, &k, &v, None)?)?;
            x = b.mlp(&x)?;
        }
        self.ln_post.forward(&x)
    }
}

pub struct Decoder {
    embedding: Embedding,
    weights: Tensor,
    pos: Tensor,
    blocks: Vec<Block>,
    ln: LayerNorm,
}

/// One decoding run: cached keys/values for each layer, for every row (beam).
pub struct DecoderState {
    self_kv: Vec<Option<(Tensor, Tensor)>>,
    cross_kv: Vec<(Tensor, Tensor)>,
    len: usize,
}

impl Decoder {
    pub fn load(vb: VarBuilder, cfg: &Config) -> Result<Self> {
        let n = cfg.d_model;
        let weights = vb.get((cfg.vocab_size, n), "embed_tokens.weight")?;
        Ok(Decoder {
            embedding: Embedding::new(weights.clone(), n),
            weights,
            pos: vb.get((cfg.max_target_positions, n), "embed_positions.weight")?,
            blocks: (0..cfg.decoder_layers)
                .map(|i| Block::load(n, cfg.decoder_attention_heads, true, vb.pp(format!("layers.{i}"))))
                .collect::<Result<_>>()?,
            ln: layer_norm(n, vb.pp("layer_norm"))?,
        })
    }

    /// Start decoding against `audio` (rows, frames, d): the audio keys/values are
    /// computed here, once.
    pub fn start(&self, audio: &Tensor) -> Result<DecoderState> {
        let mut cross_kv = Vec::with_capacity(self.blocks.len());
        for b in &self.blocks {
            let (attn, _) = b.cross.as_ref().expect("decoder blocks have cross-attention");
            cross_kv.push(attn.kv(audio)?);
        }
        Ok(DecoderState { self_kv: vec![None; self.blocks.len()], cross_kv, len: 0 })
    }

    /// Feed `tokens` (rows, n) that follow what's already in `st`; returns the logits for
    /// the last position of each row (rows, vocab).
    pub fn step(&self, tokens: &Tensor, st: &mut DecoderState) -> Result<Tensor> {
        let (rows, n) = tokens.dims2()?;
        let dev = tokens.device();
        let mut x = self.embedding.forward(tokens)?.broadcast_add(&self.pos.narrow(0, st.len, n)?)?;
        // Causal mask for the new tokens against everything so far.
        let past = st.len;
        let mask = if n > 1 {
            let total = past + n;
            let m: Vec<f32> = (0..n)
                .flat_map(|i| (0..total).map(move |j| if j > past + i { f32::NEG_INFINITY } else { 0.0 }))
                .collect();
            Some(Tensor::from_vec(m, (n, total), dev)?.to_dtype(x.dtype())?)
        } else {
            None
        };
        for (i, b) in self.blocks.iter().enumerate() {
            let h = b.attn_ln.forward(&x)?;
            let q = b.attn.q.forward(&h)?;
            let (k_new, v_new) = b.attn.kv(&h)?;
            // cache in head layout (rows, heads, tokens, hd): append along the token axis
            let (k, v) = match &st.self_kv[i] {
                Some((k0, v0)) => (Tensor::cat(&[k0, &k_new], 2)?, Tensor::cat(&[v0, &v_new], 2)?),
                None => (k_new, v_new),
            };
            x = (&x + b.attn.attend_heads(&q, &k, &v, mask.as_ref())?)?;
            st.self_kv[i] = Some((k, v));
            let (cross, ln) = b.cross.as_ref().expect("decoder blocks have cross-attention");
            let (ck, cv) = &st.cross_kv[i];
            let q = cross.q.forward(&ln.forward(&x)?)?;
            x = (&x + cross.attend_heads(&q, ck, cv, None)?)?;
            x = b.mlp(&x)?;
        }
        st.len += n;
        let last = self.ln.forward(&x.i((.., n - 1..))?)?.reshape((rows, ()))?;
        last.matmul(&self.weights.t()?)
    }

    /// Keep row `parents[i]` as row i (beam search moved on from those beams).
    pub fn reorder(st: &mut DecoderState, parents: &[u32]) -> Result<()> {
        let dev = st.cross_kv.first().map(|(k, _)| k.device().clone()).unwrap_or(candle_core::Device::Cpu);
        let idx = Tensor::new(parents, &dev)?;
        for kv in st.self_kv.iter_mut().flatten() {
            *kv = (kv.0.index_select(&idx, 0)?, kv.1.index_select(&idx, 0)?);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use candle_core::{Device, Tensor};

    /// Where the encoder's time goes, op by op (Whisper small sizes, full 30 s window).
    /// cargo test --lib op_timings -- --ignored --nocapture
    #[test]
    #[ignore]
    fn op_timings() {
        let dev = Device::Cpu;
        let x = Tensor::randn(0f32, 1.0, (1, 1500, 768), &dev).unwrap();
        let w1 = Tensor::randn(0f32, 0.02, (3072, 768), &dev).unwrap();
        let h = Tensor::randn(0f32, 1.0, (1, 1500, 3072), &dev).unwrap();
        let s = Tensor::randn(0f32, 1.0, (1, 12, 1500, 1500), &dev).unwrap();
        let q = Tensor::randn(0f32, 1.0, (1, 12, 1500, 64), &dev).unwrap();
        let ln = candle_nn::LayerNorm::new(Tensor::ones(768, candle_core::DType::F32, &dev).unwrap(), Tensor::zeros(768, candle_core::DType::F32, &dev).unwrap(), 1e-5);
        let time = |name: &str, f: &dyn Fn()| {
            f();
            let t = std::time::Instant::now();
            for _ in 0..5 {
                f();
            }
            println!("{name:28} {:7.1} ms", t.elapsed().as_secs_f64() * 1000.0 / 5.0);
        };
        time("matmul 1500x768 . 768x3072", &|| {
            x.broadcast_matmul(&w1.t().unwrap()).unwrap();
        });
        time("gelu 1500x3072", &|| {
            h.gelu().unwrap();
        });
        time("gelu_erf 1500x3072", &|| {
            h.gelu_erf().unwrap();
        });
        time("softmax 12x1500x1500", &|| {
            candle_nn::ops::softmax_last_dim(&s).unwrap();
        });
        time("q.k^T 12x1500x64", &|| {
            q.matmul(&q.transpose(2, 3).unwrap()).unwrap();
        });
        time("layer_norm 1500x768", &|| {
            candle_core::Module::forward(&ln, &x).unwrap();
        });
        time("transpose+contiguous 1500x768", &|| {
            x.reshape((1, 1500, 12, 64)).unwrap().transpose(1, 2).unwrap().contiguous().unwrap();
        });
    }
}
