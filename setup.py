import os
from setuptools import setup

this_directory = os.path.abspath(os.path.dirname(__file__))
try:
    with open(os.path.join(this_directory, 'README.md'), encoding='utf-8') as f:
        long_description = f.read()
except FileNotFoundError:
    long_description = "An Agentic Security Orchestration Engine for Web Auditing and Sandbox Analysis"

setup(
    # Note: For secure deployments, consider using pip install --require-hashes
    # to protect against dependency chain attacks and typosquatting.
    name='sentinel-cli',
    version='1.0.0',
    py_modules=['sentinel'],
    install_requires=[
        "rich==14.3.2",
        "requests==2.32.5",
        "ollama==0.6.1",
        "markdown==3.10.2",
        "packaging==26.0",
        "cryptography==46.0.5"
    ],
    entry_points={
        'console_scripts': [
            'sentinel=sentinel:main',
        ],
    },
    description='An Agentic Security Orchestration Engine for Web Auditing and Sandbox Analysis',
    long_description=long_description,
    long_description_content_type='text/markdown',
    author='Doom-pixel',
    url='https://github.com/Doom-pixel/sentinel',
    classifiers=[
        "Programming Language :: Python :: 3",
        "License :: OSI Approved :: MIT License",
        "Operating System :: OS Independent",
    ],
    python_requires='>=3.6',
)
