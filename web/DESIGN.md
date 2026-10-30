---
name: Saytwig web
description: A near-black scroll story with mint emphasis and a mint source section.
colors:
  primary: "#a5d6c3"
  action-hover: "#273c31"
  action-active: "#344d40"
  mint-text-muted: "#314c40"
  mint-rule: "#6a9b87"
  mint-link-hover: "#355746"
  neutral-ground: "#101210"
  neutral-ink: "#f1f2ec"
  neutral-muted: "#acb3a9"
  neutral-rule: "#303730"
typography:
  display:
    fontFamily: "Saytwig Lato, sans-serif"
    fontSize: "clamp(64px, 8vw, 96px)"
    fontWeight: 400
    lineHeight: 1.04
    letterSpacing: "-0.035em"
  headline:
    fontFamily: "Saytwig Lato, sans-serif"
    fontSize: "clamp(45px, 6.5vw, 86px)"
    fontWeight: 400
    lineHeight: 1.04
    letterSpacing: "-0.035em"
  body:
    fontFamily: "Saytwig Lato, sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.6
  label:
    fontFamily: "Saytwig Lato, sans-serif"
    fontSize: "12px"
    fontWeight: 400
rounded:
  action: "8px"
  waveform: "4px"
spacing:
  small: "16px"
  medium: "28px"
  section: "115px"
components:
  button-primary:
    backgroundColor: "{colors.neutral-ground}"
    textColor: "{colors.neutral-ink}"
    rounded: "{rounded.action}"
    padding: "17px 24px"
  button-primary-hover:
    backgroundColor: "{colors.action-hover}"
  button-primary-active:
    backgroundColor: "{colors.action-active}"
---

# Design System: Saytwig web

## Overview

The Saytwig website uses a near-black scroll story and a mint source section. Large regular-weight Lato headings and an animated geometric waveform carry the sequence. The original Saytwig mark, font and palette remain unchanged.

This records the website, not the native application's widgets. The animation illustrates voice becoming text. It does not represent a recording, application screenshot or measured performance.

## Colors

Mint emphasizes headings and the waveform on the dark background. The source section reverses that relationship, with dark text and actions on a mint field. Separate muted text, rule and link-hover colors support that section. Frontmatter values come from the stylesheet.

## Typography

The website serves Lato Regular as compressed WOFF2 locally with its SIL Open Font License. Sans-serif is the loading fallback. Headings use regular weight, balanced wrapping and negative tracking. Body text uses a 1.6 line height.

The display and scene-heading roles are recorded above. Controls use 12px to 14px. Privacy disclosure text uses 15px with a maximum measure of 65ch.

## Layout

With motion enabled, a 320svh story contains a sticky 100svh stage. Three scenes share that stage. A fixed header contains the mark and source link. Scene controls remain at the bottom. The source section follows in ordinary document flow and limits its inner content to 1080px.

At 760px and 520px, type, padding and control placement adapt. The narrow waveform extends past the viewport as decoration. Text remains within the viewport. The source section uses 24px side padding below 520px.

Reduced motion and the no-JavaScript fallback show the same scenes in document order. The reviewed Chromium captures cover 1440×900, 768×600, 390px-wide and 320×568 viewports. Physical phones remain unverified.

## Elevation & Depth

The design uses flat color fields, whitespace and one-pixel rules. It has no shadows or blur. Scene transitions change opacity and position without adding simulated physical materials.

## Shapes

The source action is a rounded rectangle. Narrow rounded bars form the waveform. Icons use stroked SVG geometry. The approved mark is an unchanged SVG asset, not a newly drawn approximation.

## Components

### Source action
The main action has a dark fill, light text, a 56px minimum height and an arrow. Hover and active states use the documented dark-green colors. A 150ms background transition is disabled for reduced motion.

### Header
The header contains the wordmark and source link. Motion follows the system reduced-motion preference without a page-level toggle.

### Scene controls
Voice, Words and Local are native buttons with 44px minimum heights. The active button uses mint text, an underline and aria-pressed. Inactive scene content is inert while motion is enabled, so hidden controls cannot receive keyboard focus.

### Scroll illustration
Scroll updates coalesce into one pending animation frame. A finite entrance animates the waveform. Scroll progress compresses the bars, reveals the example sentence and shows the local-recognition scene. No animation loop runs while the page is idle.

### Disclosures
Native details and summary elements contain concise privacy information. Mint-section rules separate them. Keyboard focus uses a 2px current-color outline with a 6px offset across interactive controls.

## Do's and Don'ts

### Do
- Do preserve the exact approved mark and the self-hosted font license.
- Do retain a readable static version of every animated scene.
- Do keep keyboard focus visible and inactive scene controls unreachable.
- Do label illustrative content and keep release claims tied to available artifacts.

### Don't
- Don't add perpetual animation, decorative blur or competing accent colors.
- Don't present the waveform as microphone activity or a performance measurement.
- Don't hide transcript-sharing consent or pre-release limits.
- Don't describe unreviewed devices or viewports as verified.
