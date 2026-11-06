<script lang="ts">
  import { onMount } from 'svelte';
  import { shouldAnimate } from './background-animation';
  let { enabled, recording }: { enabled: boolean; recording: boolean } = $props();
  let canvas: HTMLCanvasElement;
  let raf = 0;
  let render = () => {};
  function update() {
    cancelAnimationFrame(raf);
    if (shouldAnimate(enabled, document.hidden, matchMedia('(prefers-reduced-motion: reduce)').matches)) raf = requestAnimationFrame(render);
  }

  onMount(() => {
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const reduced = matchMedia('(prefers-reduced-motion: reduce)');
    const stars = Array.from({ length: 70 }, () => ({ x: Math.random(), y: Math.random(), size: Math.random() * 1.5 + .5 }));
    let width = 0;
    let height = 0;
    const resize = () => { width = canvas.width = innerWidth * devicePixelRatio; height = canvas.height = 552 * devicePixelRatio; };
    render = () => {
      ctx.clearRect(0, 0, width, height);
      ctx.fillStyle = 'rgba(120, 150, 190, .6)';
      for (const star of stars) {
        ctx.fillRect(star.x * width, star.y * height, star.size * devicePixelRatio, star.size * devicePixelRatio);
        star.y = (star.y + (recording ? .0018 : .00045)) % 1;
      }
      raf = requestAnimationFrame(render);
    };
    const refresh = () => { resize(); update(); };
    resize(); update();
    addEventListener('resize', resize); document.addEventListener('visibilitychange', refresh); reduced.addEventListener('change', update);
    return () => { cancelAnimationFrame(raf); removeEventListener('resize', resize); document.removeEventListener('visibilitychange', refresh); reduced.removeEventListener('change', update); };
  });

  $effect(() => {
    void enabled; void recording;
    if (canvas) update();
  });
</script>

<canvas bind:this={canvas} aria-hidden="true" class:off={!enabled}></canvas>

<style>
  canvas { position: fixed; inset: 0 auto auto 0; width: 100%; height: 552px; pointer-events: none; opacity: .6; z-index: -1; }
  canvas.off { display: none; }
</style>
