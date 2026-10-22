import { expect, test } from 'bun:test';
import { chooseRenderer, isSoftwareRenderer } from './select';

test('software renderers are recognised by name', () => {
  for (const name of [
    'ANGLE (Google, Vulkan 1.3.0 (SwiftShader Device (Subzero)), SwiftShader driver)',
    'llvmpipe (LLVM 15.0.7, 256 bits)',
    'Mesa Offscreen',
    'Microsoft Basic Render Driver',
    'Software Rasterizer',
  ]) {
    expect(isSoftwareRenderer(name)).toBe(true);
  }
  for (const name of [
    'Apple M3 Max',
    'ANGLE (Apple, ANGLE Metal Renderer: Apple M2, Unspecified Version)',
    'NVIDIA GeForce RTX 4070/PCIe/SSE2',
    'AMD Radeon Pro 5500M OpenGL Engine',
    null,
  ]) {
    expect(isSoftwareRenderer(name)).toBe(false);
  }
});

test('WebGL2 is used when it exists and runs on a GPU', () => {
  expect(chooseRenderer({ webgl2: true, gpu: 'Apple M3 Max' })).toEqual({
    kind: 'webgl2',
    gpu: 'Apple M3 Max',
  });
  // A browser that hides the name of the GPU is trusted.
  expect(chooseRenderer({ webgl2: true, gpu: null }).kind).toBe('webgl2');
});

test('Canvas2D is used when WebGL2 is absent or runs on the CPU', () => {
  expect(chooseRenderer({ webgl2: false, gpu: null }).kind).toBe('canvas2d');
  expect(chooseRenderer({ webgl2: true, gpu: 'llvmpipe (LLVM 15)' }).kind).toBe('canvas2d');
});

test('a setting overrides the choice', () => {
  expect(chooseRenderer({ webgl2: false, gpu: null }, 'webgl2').kind).toBe('webgl2');
  expect(chooseRenderer({ webgl2: true, gpu: 'Apple M3 Max' }, 'canvas2d').kind).toBe('canvas2d');
});
