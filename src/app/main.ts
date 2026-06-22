import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import "./styles.css";

/**
 * 主 WebView 只承载设置页和状态编排，桌面与 Box 运行时交互由 Rust 原生控制器接管
 */
createApp(App).use(createPinia()).mount("#app");
