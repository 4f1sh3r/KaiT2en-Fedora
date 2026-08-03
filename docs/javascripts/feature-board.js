// Filter for the feature board. Without JavaScript every row stays visible and
// only the chips are inert.
(function () {
  "use strict";

  function setup(root) {
    var chips = root.querySelectorAll(".board-chip");
    var rows = root.querySelectorAll(".board-row");
    var empty = root.querySelector(".board-empty");

    function matches(row, filter) {
      if (filter === "all") return true;
      if (filter === "help") return row.dataset.help === "yes";
      var parts = filter.split(":");
      return row.dataset[parts[0]] === parts[1];
    }

    function apply(filter) {
      var visible = 0;
      rows.forEach(function (row) {
        var show = matches(row, filter);
        row.hidden = !show;
        if (show) visible++;
      });
      if (empty) empty.hidden = visible !== 0;
    }

    chips.forEach(function (chip) {
      chip.addEventListener("click", function () {
        chips.forEach(function (other) {
          var active = other === chip;
          other.classList.toggle("is-active", active);
          other.setAttribute("aria-pressed", active ? "true" : "false");
        });
        apply(chip.dataset.filter);
      });
    });
  }

  function init() {
    document.querySelectorAll(".board").forEach(setup);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
