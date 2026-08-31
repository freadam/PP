/**
 * Host-side setup. Everything the reports actually do lives in index.html —
 * this file exists only to put the plugin somewhere the user can reach it.
 *
 * Registered from here rather than from the embed, so the button and its
 * handler outlive the view they open: a button registered inside the iframe
 * stops working the moment that iframe is closed.
 */

PluginAPI.registerHeaderButton({
  label: 'Fruit Reports',
  icon: 'insights',
  onClick: () => PluginAPI.showIndexHtmlAsView(),
});

PluginAPI.registerShortcut({
  id: 'openFruitReports',
  label: 'Open Fruit Reports',
  onExec: () => PluginAPI.showIndexHtmlAsView(),
});
