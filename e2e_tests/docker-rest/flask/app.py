from flask import Flask

app = Flask(__name__)
app.url_map.strict_slashes = False

import github  # noqa: E402,F401
import gitlab  # noqa: E402,F401
