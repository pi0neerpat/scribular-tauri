<script lang="ts">
  // Theme pane: preset choice, My Theme, and its background picture.
  import { onMount } from 'svelte';
  import { onFileDrop } from '../api';
  import { customize, presetValues, resetMyTheme, selectPreset } from '../appearance';
  import { appearance, appearanceError, chooseBackground, dropBackground, saveAppearance } from '../appearance-store';
  import type { Background } from '../types/Background';
  import type { Selection } from '../types/Selection';
  import type { ThemeValues } from '../types/ThemeValues';

  const themes: [Selection, string][] = [['default', 'Default'], ['scribularV1', 'Scribular V1'], ['sage', 'Sage'], ['myTheme', 'My Theme']];
  const pictures: [Exclude<Background, { owned: string }>, string][] = [['none', 'None'], ['stars', 'Stars'], ['dawn', 'Dawn'], ['seaGlass', 'Sea Glass'], ['linen', 'Linen']];

  let selected = $derived($appearance.selected);
  let editable = $derived(selected === 'myTheme' && !!$appearance.myTheme);
  let values = $derived(selected === 'myTheme' && $appearance.myTheme ? $appearance.myTheme.values : presetValues(selected === 'myTheme' ? 'default' : selected));
  let pictureKey = $derived(typeof values.background === 'object' ? 'owned' : values.background);
  let confirmReplace = $state(false);
  let pictureRow = $state<HTMLElement>();
  let dropping = $state(false);

  function edit(change: Partial<ThemeValues>) {
    const theme = $appearance.myTheme;
    if (!editable || !theme) return;
    void saveAppearance({ ...$appearance, myTheme: { ...theme, values: { ...theme.values, ...change } } });
  }
  function choosePicture(key: string) {
    const background = pictures.find(([value]) => value === key)?.[0];
    if (!background) return;
    // R8: Animated motion starts on whenever Stars is chosen.
    edit(background === 'stars' ? { background, starsMotion: true } : { background });
  }
  function customizeTheme(replace = false) {
    if (selected === 'myTheme') return;
    if ($appearance.myTheme && !replace) { confirmReplace = true; return; }
    confirmReplace = false;
    void saveAppearance(customize($appearance, selected, true));
  }
  const inside = (x: number, y: number) => {
    const rect = pictureRow?.getBoundingClientRect();
    return !!rect && x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
  };

  onMount(() => {
    let stop: (() => void) | undefined;
    let disposed = false;
    onFileDrop((drop) => {
      dropping = drop.type === 'over' && editable && inside(drop.x, drop.y);
      if (drop.type === 'drop' && editable && inside(drop.x, drop.y) && drop.paths.length) void dropBackground(drop.paths[0]);
    }).then((unlisten) => { if (disposed) unlisten(); else stop = unlisten; }).catch(() => {});
    return () => { disposed = true; stop?.(); };
  });
</script>

<fieldset>
  <legend>Theme</legend>
  {#each themes as [value, label]}
    <label><input type="radio" name="theme" checked={selected === value} disabled={value === 'myTheme' && !$appearance.myTheme}
      onchange={() => saveAppearance(selectPreset($appearance, value))} /> {label}</label>
  {/each}
  {#if selected !== 'myTheme'}
    <button onclick={() => customizeTheme()}>Customize</button>
  {:else}
    <button onclick={() => saveAppearance(resetMyTheme($appearance))}>Reset</button>
  {/if}
  {#if confirmReplace}
    <div role="alertdialog" aria-label="Replace My Theme?">Replace My Theme with a copy of this theme?
      <button onclick={() => (confirmReplace = false)}>Cancel</button>
      <button onclick={() => customizeTheme(true)}>Replace</button>
    </div>
  {/if}
</fieldset>

<fieldset>
  <legend>Background picture</legend>
  <div class="picture-row" class:dropping bind:this={pictureRow}>
    <select aria-label="Background picture" value={pictureKey} disabled={!editable} onchange={(e) => choosePicture(e.currentTarget.value)}>
      {#each pictures as [value, label]}<option {value}>{label}</option>{/each}
      {#if pictureKey === 'owned'}<option value="owned">Your picture</option>{/if}
    </select>
    <button disabled={!editable} onclick={() => chooseBackground()}>Choose Picture…</button>
    {#if editable}<span>or drop a picture here</span>{/if}
  </div>
  {#if values.background === 'stars'}
    <label><input type="checkbox" checked={values.starsMotion} disabled={!editable} onchange={(e) => edit({ starsMotion: e.currentTarget.checked })} /> Animated motion</label>
  {:else if values.background !== 'none'}
    <label>Blur <input type="range" min="0" max="100" value={values.blur} disabled={!editable} onchange={(e) => edit({ blur: Number(e.currentTarget.value) })} /></label>
    <label>Brightness <input type="range" min="0" max="100" value={values.brightness} disabled={!editable} onchange={(e) => edit({ brightness: Number(e.currentTarget.value) })} /></label>
  {/if}
  {#if $appearanceError}<p role="alert">{$appearanceError}</p>{/if}
</fieldset>

<style>
  .picture-row { display: flex; gap: 8px; align-items: center; padding: 6px; border: 1px dashed transparent; border-radius: 6px; }
  .picture-row.dropping { border-color: AccentColor; }
</style>
