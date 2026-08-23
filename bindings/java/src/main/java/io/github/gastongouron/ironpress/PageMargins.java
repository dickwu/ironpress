package io.github.gastongouron.ironpress;

/** Finite physical page margins measured in points. */
public record PageMargins(float top, float right, float bottom, float left) {
  /**
   * Create four equal physical margins.
   *
   * @param points finite margin in points
   * @return validated page margins
   */
  public static PageMargins uniform(float points) {
    return new PageMargins(points, points, points, points);
  }

  /**
   * Create physical margins in CSS clockwise order.
   *
   * @return validated page margins
   */
  public static PageMargins ofPoints(float top, float right, float bottom, float left) {
    return new PageMargins(top, right, bottom, left);
  }

  /** Validate directly constructed page margins. */
  public PageMargins {
    requireFinite(top, "top");
    requireFinite(right, "right");
    requireFinite(bottom, "bottom");
    requireFinite(left, "left");
  }

  private static void requireFinite(float value, String side) {
    if (!Float.isFinite(value)) {
      throw new IllegalArgumentException("Page " + side + " margin must be finite.");
    }
  }
}
