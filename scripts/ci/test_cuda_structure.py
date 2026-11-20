"""Regression checks for the static CUDA configuration guard."""
import unittest

from cuda_structure import (
    ROOT,
    check_cargo,
    check_dockerfile,
    validate,
)

GOOD_DOCKERFILE = '''\
FROM nvidia/cuda:13.0.2-devel-ubuntu24.04@sha256:aaaa AS builder-cuda
COPY Cargo.toml Cargo.lock ./
COPY examples ./examples
COPY vendor ./vendor
RUN cargo build --release --locked --no-default-features --features server,cuda,pocket-tts,reverb-tts
FROM nvidia/cuda:13.0.2-cudnn-runtime-ubuntu24.04@sha256:bbbb AS runtime-cuda
COPY --from=builder-cuda /build/target/release/SonicBoom /app/sonicboom
ENV LD_LIBRARY_PATH=/app/ort-lib:/app
RUN test -f libonnxruntime_providers_shared.so
RUN test -f libonnxruntime_providers_cuda.so
RUN ldd /app/sonicboom | grep 'not found' | grep -vE '^[[:space:]]*libcuda\\.so\\.1 => not found$'
COPY src ./src
COPY build.rs ./build.rs
'''


class CudaStructureTests(unittest.TestCase):
    def test_real_repository_passes(self):
        validate(ROOT)

    def test_missing_cuda_feature(self):
        with self.assertRaisesRegex(ValueError, 'missing the `cuda` feature'):
            check_cargo({'features': {}})

    def test_dockerfile_missing_stage(self):
        with self.assertRaisesRegex(ValueError, 'builder-cuda'):
            check_dockerfile(GOOD_DOCKERFILE.replace('AS builder-cuda', 'AS builder'))

    def test_dockerfile_unpinned_cuda_image(self):
        broken = GOOD_DOCKERFILE.replace('@sha256:aaaa', '')
        with self.assertRaisesRegex(ValueError, 'digest-pinned'):
            check_dockerfile(broken)

    def test_dockerfile_missing_copy_input(self):
        broken = GOOD_DOCKERFILE.replace('COPY vendor ./vendor\n', '')
        with self.assertRaisesRegex(ValueError, 'never COPYs required inputs'):
            check_dockerfile(broken)


if __name__ == '__main__':
    unittest.main()
