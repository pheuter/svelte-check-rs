<script lang="ts">
  let {
    data = $bindable([]),
    value_mode = $bindable(DEFAULTS.treemap.value_mode),
    // descending (unlike Sunburst's input-order default): squarified tiling
    // reads best with the largest cell top-left and smallest bottom-right
    sort = `descending`,
    level_lighten = 0,
    min_fraction = $bindable(DEFAULTS.treemap.min_fraction),
    other_label = `Other`,
    max_depth = $bindable(DEFAULTS.treemap.max_depth),
    padding_inner = $bindable(DEFAULTS.treemap.padding_inner),
    padding_top = $bindable(DEFAULTS.treemap.padding_top),
    padding_outer = $bindable(DEFAULTS.treemap.padding_outer),
    show_labels = $bindable(DEFAULTS.treemap.show_labels),
    label_text = $bindable(DEFAULTS.treemap.label_text),
    label_formatter,
    label_fit = `shrink`,
    label_min_font_size = 6,
    label_max_font_size,
    parent_label_font_size = 14,
    zoom_on_click = $bindable(DEFAULTS.treemap.zoom_on_click),
    zoom_root_id = $bindable(null),
    show_breadcrumbs = $bindable(DEFAULTS.treemap.show_breadcrumbs),
    color_values,
    color_scale = SCALE_DEFAULTS.scheme,
    color_range,
    colorbar = {},
    export_buttons = true,
    export_filename = `treemap`,
    tween,
    value_format = `,`,
    padding = DEFAULT_PADDING,
    legend = {},
    show_legend = false,
    tooltip,
    cell_content,
    hovered = $bindable(false),
    change = () => {},
    on_node_click,
    on_node_hover,
    on_zoom,
    show_controls = $bindable(true),
    controls_open = $bindable(false),
    controls_toggle_props,
    controls_pane_props,
    fullscreen = $bindable(false),
    fullscreen_toggle = true,
    children,
    header_controls,
    controls_extra,
    ...rest
  }: HTMLAttributes<HTMLDivElement> &
    Omit<BasePlotProps, `change`> & {
      data?: TreemapNode<Metadata> | TreemapNode<Metadata>[]
      value_mode?: SunburstValueMode
      sort?: SunburstSort // default 'descending' (largest top-left); 'none' keeps input order
      level_lighten?: number
      // Aggregate sibling cells below this fraction of the total into one 'Other'
      // cell per parent (only when >= 2 qualify); 0 disables
      min_fraction?: number
      other_label?: string
      max_depth?: number // levels shown below the current zoom root (0 = all)
      padding_inner?: number // px gap between sibling cells
      padding_top?: number // px header strip on branch cells (0 = no headers)
      padding_outer?: number // px inset of children within their parent (plotly marker.pad)
      show_labels?: boolean
      label_text?: SunburstLabelText // what labels display (plotly textinfo equivalent)
      // Structured multiline labels. Unlike cell_content, this keeps built-in
      // hover/focus/click and tooltip behavior on the underlying cell.
      label_formatter?: TreemapLabelFormatter<Metadata>
      label_fit?: TreemapLabelFit // shrink-to-fit (default), hide, or clip at max size
      label_min_font_size?: number // px floor used by shrink mode
      label_max_font_size?: number // px ceiling for leaf/cutoff labels
      parent_label_font_size?: number // px size/ceiling for branch header labels
      zoom_on_click?: boolean
      zoom_root_id?: string | number | null // id of the cell the view is rooted on
      show_breadcrumbs?: boolean // clickable ancestor trail when zoomed
      // Color cells by a numeric metric (continuous colormap) instead of categorical
      // inheritance; return null to keep a cell's categorical color
      color_values?: (rect: PositionedArc<Metadata>) => number | null
      color_scale?: D3InterpolateName
      color_range?: Vec2 // defaults to the metric's [min, max]
      colorbar?: ComponentProps<typeof ColorBar> | null // null hides it
      export_buttons?: boolean // SVG/PNG download buttons in the controls pane
      export_filename?: string
      // Zoom transition timing (resizes/data swaps snap instantly, plotly-style).
      // interpolate is not overridable: the component's rect interpolator also
      // handles rect-array length changes on data swaps (default would throw)
      tween?: Omit<TweenOptions<Rect[]>, `interpolate`>
      value_format?: string
      padding?: Sides
      legend?: LegendConfig | null
      show_legend?: boolean
      tooltip?: Snippet<[TreemapNodeHandlerProps<Metadata>]>
      // Fully replace the default cell rect + labels. NOTE: this also replaces the
      // built-in hover/focus/click + tooltip wiring, so re-implement any
      // interactivity you need inside the snippet.
      cell_content?: Snippet<[{ arc: PositionedArc<Metadata>; rect: Rect }]>
      change?: (data: TreemapNodeHandlerProps<Metadata> | null) => void
      on_node_click?: (
        data: TreemapNodeHandlerProps<Metadata> & { event: MouseEvent | KeyboardEvent },
      ) => void
      on_node_hover?: (
        data: (TreemapNodeHandlerProps<Metadata> & { event: MouseEvent | FocusEvent }) | null,
      ) => void
      on_zoom?: (data: { root: TreemapNodeHandlerProps<Metadata> | null }) => void
      header_controls?: Snippet<[{ height: number; width: number; fullscreen: boolean }]>
      controls_extra?: Snippet<[{ zoom_root_id: string | number | null }]>
    } = $props()

</script>
