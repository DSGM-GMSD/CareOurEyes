// Botón de descarga según el sistema de quien visita, con el archivo del último
// release de GitHub. Si algo falla, los enlaces llevan a la página de releases.
(function () {
  var REPO = "DSGM-GMSD/CareOurEyes";
  var main = document.getElementById("dl-main");
  var version = document.getElementById("dl-version");
  if (!main) return;

  var ua = navigator.userAgent || "";
  var platform = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
  var os = /Win/i.test(platform + ua) ? "windows"
    : /Mac|iPhone|iPad/i.test(platform + ua) ? "macos"
    : /Linux|X11/i.test(platform + ua) ? "linux" : "";

  // Archivo de cada sistema dentro del release.
  var pick = {
    windows: function (n) { return /-setup\.exe$/i.test(n); },
    linux: function (n) { return /\.deb$/i.test(n); },
    macos: function (n) { return /-macos\.dmg$/i.test(n); }
  };

  if (os && main.dataset[os]) {
    main.querySelector("span").textContent = main.dataset[os];
  }

  fetch("https://api.github.com/repos/" + REPO + "/releases?per_page=10")
    .then(function (r) { return r.ok ? r.json() : []; })
    .then(function (list) {
      var rel = (list || []).filter(function (r) { return !r.draft; })[0];
      if (!rel) return;
      if (version) version.textContent = rel.tag_name;
      var assets = rel.assets || [];
      function url(kind) {
        var a = assets.filter(function (x) { return pick[kind](x.name); })[0];
        return a ? a.browser_download_url : null;
      }
      if (os) {
        var u = url(os);
        if (u) main.href = u;
      }
      ["windows", "linux", "macos"].forEach(function (kind) {
        var link = document.getElementById("dl-" + kind);
        var u = url(kind);
        if (link && u) link.href = u;
      });
    })
    .catch(function () { /* se queda el enlace a releases */ });
})();
