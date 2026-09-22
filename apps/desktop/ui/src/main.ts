/**
 * The window's entry point: styles, then the app. Everything the user sees is `App.svelte`.
 */

import "./styles/tokens.css";
import "./styles/oc.css";

import { mount } from "svelte";

import App from "./App.svelte";

const target = document.getElementById("app");
if (target !== null) mount(App, { target });
