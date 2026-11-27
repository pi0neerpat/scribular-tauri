<script lang="ts">
  import { onMount } from 'svelte';
  import { starsState } from './background-animation';
  let { motion, recording }: { motion: boolean; recording: boolean } = $props();
  let canvas: HTMLCanvasElement;
  let raf = 0;
  let draw = (_advance: boolean) => {};
  const reducedMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches;
  let ready = false;
  function update() {
    cancelAnimationFrame(raf);
    // No 2D context means nothing to draw, so never start the frame loop.
    if (!ready) return;
    const state = starsState('stars', motion, recording, document.hidden, reducedMotion());
    if (state.animate) {
      const frame = () => { draw(true); raf = requestAnimationFrame(frame); };
      raf = requestAnimationFrame(frame);
    } else {
      draw(false);
    }
  }

  onMount(() => {
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const reduced = matchMedia('(prefers-reduced-motion: reduce)');
    const stars = Array.from({ length: 70 }, () => ({ x: Math.random(), y: Math.random(), size: Math.random() * 1.5 + .5 }));
    let width = 0;
    let height = 0;
    const resize = () => { width = canvas.width = innerWidth * devicePixelRatio; height = canvas.height = innerHeight * devicePixelRatio; draw(false); };
    draw = (advance) => {
      const { speed } = starsState('stars', motion, recording, false, false);
      ctx.clearRect(0, 0, width, height);
      ctx.fillStyle = 'rgba(120, 150, 190, .6)';
      for (const star of stars) {
        ctx.fillRect(star.x * width, star.y * height, star.size * devicePixelRatio, star.size * devicePixelRatio);
        if (advance) star.y = (star.y + speed) % 1;
      }
    };
    ready = true;
    const refresh = () => { resize(); update(); };
    resize(); update();
    addEventListener('resize', resize); document.addEventListener('visibilitychange', refresh); reduced.addEventListener('change', update);
    return () => { ready = false; cancelAnimationFrame(raf); removeEventListener('resize', resize); document.removeEventListener('visibilitychange', refresh); reduced.removeEventListener('change', update); };
  });

  $effect(() => {
    void motion; void recording;
    if (canvas) update();
    return () => cancelAnimationFrame(raf);
  });
</script>

<canvas bind:this={canvas} aria-hidden="true"></canvas>

<style>
  canvas { position: fixed; inset: 0; width: 100%; height: 100vh; pointer-events: none; opacity: .6; z-index: -1; }
</style>
