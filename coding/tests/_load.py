import importlib.util, sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def script(relative):
 path=ROOT/relative; spec=importlib.util.spec_from_file_location(path.stem.replace('-','_'),path); module=importlib.util.module_from_spec(spec); assert spec and spec.loader; sys.modules[spec.name]=module; spec.loader.exec_module(module); return module
