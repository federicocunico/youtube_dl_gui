<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { Film } from '@lucide/svelte';
  export let url: string | undefined = undefined;
  export let large = false;
  let source = '';
  let element: HTMLDivElement;
  onMount(() => {
    const observer = new IntersectionObserver(async (entries) => {
      if (!entries.some((e) => e.isIntersecting)) return;
      observer.disconnect();
      if (url?.startsWith('data:image/')) source = url;
      else if (url)
        try {
          source = await invoke<string>('thumbnail', { url });
        } catch {}
    });
    observer.observe(element);
    return () => observer.disconnect();
  });
</script>

<div class="thumb" class:large bind:this={element}>
  {#if source}<img src={source} alt="" loading="lazy" />{:else}<Film
      size={large ? 44 : 24}
      strokeWidth={1.3}
    />{/if}<slot />
</div>

<style>
  .thumb {
    aspect-ratio: 16/9;
    display: grid;
    place-items: center;
    position: relative;
    overflow: hidden;
    background: linear-gradient(135deg, #252e36, #172229);
    color: #788d92;
    border-radius: 9px;
    min-width: 75px;
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    position: absolute;
  }
  .large {
    width: 100%;
    border-radius: 12px;
  }
</style>
