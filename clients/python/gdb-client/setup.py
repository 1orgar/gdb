from setuptools import setup, find_packages

setup(
    name="gdb-client",
    version="0.5.1",
    packages=find_packages(),
    install_requires=[
        "requests>=2.28.0",
    ],
    extras_require={
        "all": [
            "pyarrow>=14.0.0",
            "polars>=0.20.0",
            "networkx>=3.0",
            "numpy>=1.24.0",
        ],
        "flight": ["pyarrow>=14.0.0"],
        "analytics": ["polars>=0.20.0", "networkx>=3.0"],
    },
    python_requires=">=3.8",
)
