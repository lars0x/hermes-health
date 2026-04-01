// Hermes Health - Client-side JS (chart init + autocomplete + UI helpers)

// Initialize charts after HTMX swaps or page load
function initCharts() {
  document.querySelectorAll('.chart-container[data-chart]').forEach(function(el) {
    if (el._uplot) { el._uplot.destroy(); }
    try {
      var data = JSON.parse(el.dataset.chart);
      if (!data.timestamps || data.timestamps.length < 1) {
        el.innerHTML = '<div style="padding:40px;text-align:center;color:#6b6b6b;font-size:13px;">No data points yet</div>';
        return;
      }

      // For a single point, add padding timestamps so uPlot has a visible x range
      if (data.timestamps.length === 1) {
        var t = data.timestamps[0];
        var dayInSec = 86400;
        data.timestamps = [t - 30 * dayInSec, t, t + 30 * dayInSec];
        data.values = [null, data.values[0], null];
      }

      var regression = null;
      try { regression = JSON.parse(el.dataset.regression); } catch(e) {}

      var interventions = [];
      try { interventions = JSON.parse(el.dataset.interventions); } catch(e) {}

      var unit = el.dataset.unit || '';

      // Compute Y range with padding
      var allVals = data.values.filter(function(v) { return v != null; });
      if (data.reference_low != null) allVals.push(data.reference_low);
      if (data.reference_high != null) allVals.push(data.reference_high);
      if (data.optimal_low != null) allVals.push(data.optimal_low);
      if (data.optimal_high != null) allVals.push(data.optimal_high);
      var yMin = Math.min.apply(null, allVals);
      var yMax = Math.max.apply(null, allVals);
      var yPad = (yMax - yMin) * 0.15 || 10;

      // Build series data
      var seriesData = [data.timestamps, data.values];
      var series = [
        {},
        {
          label: unit,
          stroke: '#378ADD',
          width: 2,
          spanGaps: false,
          points: { size: 8, fill: '#378ADD' }
        }
      ];

      // Add regression line if available (need at least 2 real non-null values)
      var realPoints = data.values.filter(function(v) { return v != null; }).length;
      if (regression && regression.slope != null && realPoints >= 2) {
        var firstReal = data.values.findIndex(function(v) { return v != null; });
        var t0 = data.timestamps[firstReal];
        var v0 = data.values[firstReal];
        var slopePerSec = regression.slope / (365.25 * 86400);
        var regVals = data.timestamps.map(function(t) {
          return v0 + slopePerSec * (t - t0);
        });
        seriesData.push(regVals);
        series.push({
          label: 'Trend',
          stroke: '#E24B4A',
          width: 1,
          dash: [6, 4],
          points: { show: false }
        });
      }

      var opts = {
        width: el.clientWidth || 600,
        height: 220,
        series: series,
        scales: {
          y: { range: [yMin - yPad, yMax + yPad] }
        },
        axes: [
          { stroke: '#6b6b6b', font: '11px sans-serif', grid: { stroke: '#e0e0e0', width: 0.5 } },
          {
            stroke: '#6b6b6b', font: '11px sans-serif',
            grid: { stroke: '#e0e0e0', width: 0.5 },
            values: function(u, vals) { return vals.map(function(v) { return v == null ? '' : v.toFixed(0); }); }
          }
        ],
        hooks: {
          drawAxes: [function(u) {
            var ctx = u.ctx;
            var lft = u.bbox.left;
            var rgt = lft + u.bbox.width;
            var scaleY = u.scales.y;

            function yToPos(val) {
              return u.valToPos(val, 'y', true);
            }

            // Reference range fill (light red for out-of-range zones)
            var topY = yToPos(scaleY.max || yMax + yPad);
            var botY = yToPos(scaleY.min || yMin - yPad);
            ctx.fillStyle = 'rgba(226,75,74,0.08)';
            if (data.reference_high != null) {
              // Above reference high = red zone
              var refHighY = yToPos(data.reference_high);
              ctx.fillRect(lft, Math.min(topY, refHighY), rgt - lft, Math.abs(refHighY - topY));
            }
            if (data.reference_low != null) {
              // Below reference low = red zone
              var refLowY = yToPos(data.reference_low);
              ctx.fillRect(lft, Math.min(refLowY, botY), rgt - lft, Math.abs(botY - refLowY));
            }

            // Optimal range fill (light green)
            ctx.fillStyle = 'rgba(29,158,117,0.1)';
            if (data.optimal_low != null && data.optimal_high != null) {
              var optLowY = yToPos(data.optimal_low);
              var optHighY = yToPos(data.optimal_high);
              ctx.fillRect(lft, Math.min(optLowY, optHighY), rgt - lft, Math.abs(optHighY - optLowY));
            } else if (data.optimal_low != null) {
              // One-sided: everything above optimal_low is green (e.g., HDL: higher is better)
              var optLowY = yToPos(data.optimal_low);
              ctx.fillRect(lft, Math.min(topY, optLowY), rgt - lft, Math.abs(optLowY - topY));
            } else if (data.optimal_high != null) {
              // One-sided: everything below optimal_high is green (e.g., LDL: lower is better)
              var optHighY = yToPos(data.optimal_high);
              ctx.fillRect(lft, Math.min(optHighY, botY), rgt - lft, Math.abs(botY - optHighY));
            }

            // Intervention markers (purple dashed vertical lines)
            interventions.forEach(function(iv) {
              var x = u.valToPos(iv.timestamp, 'x', true);
              if (x >= lft && x <= rgt) {
                ctx.save();
                ctx.strokeStyle = '#534AB7';
                ctx.lineWidth = 1;
                ctx.setLineDash([3, 3]);
                ctx.globalAlpha = 0.6;
                ctx.beginPath();
                ctx.moveTo(x, u.bbox.top);
                ctx.lineTo(x, u.bbox.top + u.bbox.height);
                ctx.stroke();
                // Label
                ctx.fillStyle = '#534AB7';
                ctx.font = '9px sans-serif';
                ctx.fillText(iv.name, x + 3, u.bbox.top + 12);
                ctx.restore();
              }
            });
          }]
        }
      };

      el._uplot = new uPlot(opts, seriesData, el);
    } catch(e) {
      console.error('Chart init error:', e);
    }
  });
}

// Autocomplete selection
function selectBiomarker(code, name, unit) {
  var search = document.getElementById('biomarker-search');
  if (search) search.value = name;
  var hidden = document.getElementById('biomarker-code');
  if (hidden) hidden.value = code;
  var unitInput = document.getElementById('obs-unit');
  if (unitInput && unit) unitInput.value = unit;
  // Clear dropdown
  var dropdown = document.getElementById('autocomplete-results');
  if (dropdown) dropdown.innerHTML = '';
}

// Pill button toggle
function updatePills(clicked) {
  var group = clicked.closest('.pill-group');
  if (!group) return;
  group.querySelectorAll('.pill-btn').forEach(function(btn) {
    btn.classList.remove('active');
  });
  clicked.classList.add('active');
}

// Close autocomplete on outside click
document.addEventListener('click', function(e) {
  var wrapper = document.querySelector('.autocomplete-wrapper');
  if (wrapper && !wrapper.contains(e.target)) {
    var dropdown = document.getElementById('autocomplete-results');
    if (dropdown) dropdown.innerHTML = '';
  }
});

// Initialize on page load
document.addEventListener('DOMContentLoaded', initCharts);

// Re-initialize after HTMX swaps
document.addEventListener('htmx:afterSettle', initCharts);

// Handle chart resize
var resizeTimer;
window.addEventListener('resize', function() {
  clearTimeout(resizeTimer);
  resizeTimer = setTimeout(initCharts, 200);
});
