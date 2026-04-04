process <- function(data, y="xxxxx", x="xxxxx", m="xxxxx", w="xxxxx", z="xxxxx", cov="xxxxx", model=999, boot=5000, conf=95, center=0, seed=-999, outscreen=1, progress=0, save=0, ...) {
  cat("PROCESSv50 mock run\n")
  cat(sprintf("model=%s\n", model))
  cat(sprintf("y=%s x=%s\n", y, x))
  cat(sprintf("m=%s w=%s z=%s cov=%s\n", paste(m, collapse=","), paste(w, collapse=","), paste(z, collapse=","), paste(cov, collapse=",")))
  cat(sprintf("boot=%s conf=%s center=%s seed=%s\n", boot, conf, center, seed))
  cat(sprintf("rows=%s cols=%s\n", nrow(data), ncol(data)))
  invisible(list(model=model, outcome=y, predictor=x, rows=nrow(data), columns=ncol(data)))
}
