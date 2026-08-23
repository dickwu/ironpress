package io.github.gastongouron.ironpress;

/** A positive finite custom page size measured in points. */
public record PageDimensions(float width, float height) {
  /**
   * Create a custom physical page size.
   *
   * @param width positive finite width in points
   * @param height positive finite height in points
   * @return validated page dimensions
   */
  public static PageDimensions ofPoints(float width, float height) {
    return new PageDimensions(width, height);
  }

  /** Validate directly constructed page dimensions. */
  public PageDimensions {
    if (!Float.isFinite(width) || width <= 0) {
      throw new IllegalArgumentException("Page width must be positive and finite.");
    }
    if (!Float.isFinite(height) || height <= 0) {
      throw new IllegalArgumentException("Page height must be positive and finite.");
    }
  }
}
